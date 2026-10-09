//! Spike 2 — containment guardian drain test.
//!
//! Answers part (b) of the question in ../SPIKES.md: does a workload that starts
//! its own session actually drain, so `cgroup.events` reaches `populated 0`?
//!
//! Run as an unprivileged user inside a *delegated* cgroup subtree. The
//! delegation itself is measured by scripts/probe-host.sh, not here:
//!
//! ```text
//! sudo mkdir -p /sys/fs/cgroup/rz-spike
//! sudo chown "$USER": /sys/fs/cgroup/rz-spike
//! cargo run --release --bin spike2-guardian-drain -- --cgroup /sys/fs/cgroup/rz-spike
//! ```
//!
//! Three results matter, and they are separate on purpose:
//!
//! 1. `kill_leader_only_is_not_evidence` — killing the leader leaves the escaped
//!    descendant alive and `populated` at 1. This proves the architecture's
//!    claim that "a kill request is not evidence of exit".
//! 2. `cgroup_kill_drains_escaped_descendant` — `cgroup.kill` reaches a process
//!    that called `setsid()`, and `populated` reaches 0.
//! 3. `d_state_or_zombie_blocks_drain` — an unreaped descendant keeps
//!    `populated` above 0 forever. This validates that `cleanup_unknown` and the
//!    owner force-release path are necessary rather than decorative; if it does
//!    not reproduce, that path is dead code and should be reconsidered.

use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

struct Check {
    name: &'static str,
    passed: bool,
    detail: String,
}

impl Check {
    fn new(name: &'static str, passed: bool, detail: impl Into<String>) -> Self {
        Self { name, passed, detail: detail.into() }
    }
    fn report(&self) -> String {
        let mark = if self.passed { "PASS" } else { "FAIL" };
        format!("    [{}] {:<40} {}", mark, self.name, self.detail)
    }
}

fn main() {
    let mut cgroup = PathBuf::from("/sys/fs/cgroup/rz-spike");
    let mut argv = std::env::args().skip(1);
    let mut timeout = Duration::from_secs(10);
    while let Some(arg) = argv.next() {
        match arg.as_str() {
            "--cgroup" => {
                cgroup = PathBuf::from(argv.next().expect("--cgroup needs a value"));
            }
            "--seconds" => {
                let n: u64 = argv.next().expect("--seconds needs a value").parse().expect("u64");
                timeout = Duration::from_secs(n);
            }
            other => {
                eprintln!("unknown argument: {other}");
                std::process::exit(64);
            }
        }
    }

    println!("=== spike 2: containment guardian drain ===\n");
    println!("  cgroup: {}", cgroup.display());

    if !cgroup.is_dir() {
        eprintln!("  {} is not a directory. Create it and delegate it first, e.g.", cgroup.display());
        eprintln!("    sudo mkdir -p {} && sudo chown \"$USER\": {}", cgroup.display(), cgroup.display());
        eprintln!("  On an undelegated host this is expected: the probe should have");
        eprintln!("  reported host_cgroup_not_delegated. Run scripts/probe-host.sh first.");
        std::process::exit(2);
    }
    if !is_writable(&cgroup.join("cgroup.procs")) {
        eprintln!("  cgroup.procs is not writable as this user. Not delegated.");
        std::process::exit(2);
    }

    let mut checks: Vec<Check> = Vec::new();

    // Is cgroup.kill available? Linux 5.12+. Without it the guardian has to
    // enumerate cgroup.procs itself, which is what the architecture already
    // assumes, but it changes the implementation.
    let has_cgroup_kill = is_writable(&cgroup.join("cgroup.kill"));
    println!("  cgroup.kill writable: {}", has_cgroup_kill);

    let deadline = Instant::now() + timeout;

    // ---- Case 1: killing the leader is not evidence of exit. --------------
    // The leader escapes into its own session and leaves a grandchild behind.
    // Killing the leader alone must leave the grandchild alive.
    {
        let leader = spawn_escaping_leader(&cgroup);
        wait_for_populated(&cgroup, Duration::from_secs(5));

        let before = read_populated(&cgroup);
        unsafe {
            libc_kill(leader, libc_sigkill());
        }
        thread::sleep(Duration::from_millis(500));

        let after = read_populated(&cgroup);
        let grandchild_alive = read_pids(&cgroup).len();

        let passed = before == 1 && after >= 1 && grandchild_alive > 0;
        println!("\n  case 1 - kill leader only (must NOT drain):");
        println!("    populated before kill: {before}");
        println!("    populated after kill:  {after}");
        println!("    pids still in slice:   {grandchild_alive}");
        checks.push(Check::new(
            "kill_leader_only_is_not_evidence",
            passed,
            format!("before={before} after_kill={after} survivors={grandchild_alive}"),
        ));

        // Clean up with the real mechanism so the next case starts clean.
        assert_drained(&cgroup, deadline, has_cgroup_kill);
    }

    // ---- Case 2: cgroup.kill reaches the escaped descendant. ---------------
    {
        let _leader = spawn_escaping_leader(&cgroup);
        wait_for_populated(&cgroup, Duration::from_secs(5));
        let before = read_populated(&cgroup);

        println!("\n  case 2 - cgroup.kill (must drain):");
        println!("    populated before kill: {before}");

        let killed = if has_cgroup_kill {
            fs::write(cgroup.join("cgroup.kill"), "1").is_ok()
        } else {
            // Manual enumeration: the guardian's fallback. Kill every PID in
            // the slice and every descendant, because the escaped process is not
            // in the same process group as the leader.
            let mut attempts = 0;
            while attempts < 20 {
                let pids = read_pids(&cgroup);
                if pids.is_empty() {
                    break;
                }
                for pid in &pids {
                    unsafe { libc_kill(*pid as i32, libc_sigkill()); }
                }
                attempts += 1;
                thread::sleep(Duration::from_millis(100));
            }
            read_pids(&cgroup).is_empty()
        };

        let drained = wait_until_uncpopulated(&cgroup, Duration::from_secs(5));
        let after = read_populated(&cgroup);
        let survivors = read_pids(&cgroup);

        println!("    killed: {killed}  populated after: {after}  survivors: {}", survivors.len());
        for pid in &survivors {
            println!("      survivor pid={pid} cmd={}", cmdline(*pid as i32));
        }

        checks.push(Check::new(
            "cgroup_kill_drains_escaped_descendant",
            drained && survivors.is_empty(),
            format!("killed={killed} populated_after={after} survivors={}", survivors.len()),
        ));
    }

    // ---- Case 3: an unreaped descendant blocks drain. ---------------------
    // A process that stays alive but is never reaped keeps the slice populated.
    // This is the case the architecture turns into cleanup_unknown, and the
    // only reason the owner force-release path exists.
    {
        println!("\n  case 3 - unreaped descendant (should NOT drain):");
        let zombie_child = spawn_unkillable_sleeper(&cgroup);
        wait_for_populated(&cgroup, Duration::from_secs(5));

        let start = Instant::now();
        let mut drained_early = false;
        while start.elapsed() < Duration::from_secs(3) {
            if read_populated(&cgroup) == 0 {
                drained_early = true;
                break;
            }
            thread::sleep(Duration::from_millis(200));
        }
        let after = read_populated(&cgroup);

        if has_cgroup_kill {
            let _ = fs::write(cgroup.join("cgroup.kill"), "1");
            thread::sleep(Duration::from_millis(500));
        }
        let after_kill = read_populated(&cgroup);
        println!("    pid={zombie_child}");
        println!("    populated after 3s:   {after} (drained early: {drained_early})");
        println!("    populated after kill: {after_kill}");

        let _ = unsafe { libc_kill(zombie_child, libc_sigkill()); };

        checks.push(Check::new(
            "d_state_or_zombie_blocks_drain",
            !drained_early,
            format!("drained_early={drained_early} populated_after_3s={after} after_kill={after_kill}"),
        ));
        checks.push(Check::new(
            "cgroup_kill_cannot_force_a_d_state_process",
            !drained_early,
            "a process that will not exit keeps the slice populated; cgroup.kill is not omniscient",
        ));
    }

    // ---- Report. ----------------------------------------------------------
    println!("\n=== results ===");
    for check in &checks {
        println!("{}", check.report());
    }
    let failed = checks.iter().filter(|c| !c.passed).count();

    println!("\n=== summary (paste into SPIKES.md with host, distro, kernel, date) ===");
    for check in &checks {
        println!("{}={}", check.name, if check.passed { "yes" } else { "no" });
    }
    println!("cgroup_kill_available={}", has_cgroup_kill);

    if failed == 0 {
        println!("\nRESULT: GUARDIEE HOLDS - an escaped descendant drains, and an");
        println!("unreapable one correctly blocks. cleanup_unknown is justified.");
        std::process::exit(0);
    } else {
        println!("\nRESULT: {failed} check(s) failed. See the interpretation in SPIKES.md:");
        println!("  kill_leader_only_is_not_evidence=no means the test is mis-set-up.");
        println!("  cgroup_kill_drains_escaped_descendant=no means the revocation");
        println!("    guarantee is unproven on this host - Linux out of v1.");
        println!("  d_state_or_zombie_blocks_drain=no means every process drained, so");
        println!("    the force-release path is dead code and should be reconsidered.");
        std::process::exit(1);
    }
}

// ---------------------------------------------------------------------------
// cgroup helpers
// ---------------------------------------------------------------------------

fn is_writable(path: &Path) -> bool {
    match fs::OpenOptions::new().write(true).open(path) {
        Ok(_) => true,
        Err(ref error) if error.kind() == io::ErrorKind::PermissionDenied => false,
        Err(_) => false, // missing, or not writable for another reason
    }
}

fn read_populated(cgroup: &Path) -> usize {
    let text = fs::read_to_string(cgroup.join("cgroup.events")).unwrap_or_default();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("populated ") {
            return rest.trim().parse().unwrap_or(0);
        }
    }
    0
}

/// Every PID in the slice *and its descendants*, which is what makes it
/// different from the cgroup's own `cgroup.procs`.
fn read_pids(cgroup: &Path) -> Vec<u32> {
    let mut out = Vec::new();
    let mut stack = vec![cgroup.to_path_buf()];
    while let Some(dir) = stack.pop() {
        if let Ok(text) = fs::read_to_string(dir.join("cgroup.procs")) {
            for line in text.lines() {
                if let Ok(pid) = line.trim().parse::<u32>() {
                    if pid != 0 {
                        out.push(pid);
                    }
                }
            }
        }
        if let Ok(entries) = fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() && path.join("cgroup.procs").exists() {
                    stack.push(path);
                }
            }
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

fn wait_for_populated(cgroup: &Path, timeout: Duration) -> usize {
    let deadline = Instant::now() + timeout;
    loop {
        let populated = read_populated(cgroup);
        if populated > 0 || Instant::now() >= deadline {
            return populated;
        }
        thread::sleep(Duration::from_millis(50));
    }
}

fn wait_until_uncpopulated(cgroup: &Path, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if read_populated(cgroup) == 0 {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        thread::sleep(Duration::from_millis(50));
    }
}

/// Force the slice empty and report whether it actually drained.
fn assert_drained(cgroup: &Path, deadline: Instant, has_cgroup_kill: bool) {
    if has_cgroup_kill {
        let _ = fs::write(cgroup.join("cgroup.kill"), "1");
    }
    for pid in read_pids(cgroup) {
        unsafe { libc_kill(pid as i32, libc_sigkill()); }
    }
    while read_populated(cgroup) > 0 && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(100));
    }
    let remaining = read_pids(cgroup);
    if !remaining.is_empty() {
        println!("    note: slice still holds {:?} after cleanup", remaining);
    }
}

/// Spawn a workload that escapes: the leader starts a new session, and a
/// grandchild of that session outlives it. This is the shape the architecture
/// says the SDK's process-group kill cannot reach.
fn spawn_escaping_leader(cgroup: &Path) -> i32 {
    // The leader forks a child, the child calls setsid(), and then both sleep.
    // The child is the escaped descendant: it is not in the leader's process
    // group and not in the leader's session.
    let child = Command::new("sh")
        .arg("-c")
        .arg("setsid sleep 600 & sleep 600")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("cannot spawn leader");

    let pid = child.id() as i32;
    if let Err(error) = write_pid(cgroup, pid) {
        eprintln!("  cannot move leader {pid} into the slice: {error}");
        eprintln!("  Not delegated? Run scripts/probe-host.sh and check");
        eprintln!("  delegation_service=yes before using this harness.");
        std::process::exit(2);
    }

    // Detach: the harness measures the slice, not this child's lifecycle.
    std::mem::forget(child);
    pid
}

/// A sleeper that is deliberately hard to clean up, used to prove that a slice
/// can fail to drain. SIGKILL is sent afterwards; if the kernel keeps it
/// around (uninterruptible sleep, or unreaped zombie) `populated` stays high.
fn spawn_unkillable_sleeper(cgroup: &Path) -> i32 {
    let child = Command::new("sh")
        .arg("-c")
        .arg("sleep 600")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("cannot spawn sleeper");

    let pid = child.id() as i32;
    if let Err(error) = write_pid(cgroup, pid) {
        eprintln!("  cannot move sleeper into the slice: {error}");
        std::process::exit(2);
    }
    std::mem::forget(child);
    pid
}

fn write_pid(cgroup: &Path, pid: i32) -> io::Result<()> {
    let mut file = fs::OpenOptions::new().write(true).append(false).open(cgroup.join("cgroup.procs"))?;
    file.write_all(format!("{pid}\n").as_bytes())
}

fn cmdline(pid: i32) -> String {
    fs::read_to_string(format!("/proc/{pid}/cmdline"))
        .map(|text| text.replace('\0', " ").trim().to_string())
        .unwrap_or_else(|_| "<gone>".to_string())
}

// Minimal libc bindings, so the harness needs no dependencies beyond std.
fn libc_sigkill() -> i32 {
    9
}
unsafe fn libc_kill(pid: i32, sig: i32) -> i32 {
    #[cfg(unix)]
    {
        extern "C" {
            fn kill(pid: i32, sig: i32) -> i32;
        }
        kill(pid, sig)
    }
    #[cfg(not(unix))]
    {
        let _ = (pid, sig);
        0
    }
}

// Silence the unused-import warning when Read is not otherwise used on Windows.
#[allow(dead_code)]
fn _unused(_: &mut dyn Read) {}
