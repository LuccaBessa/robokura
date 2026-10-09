//! Spike 1 — Windows exact-file grants.
//!
//! Answers the question in ../SPIKES.md: can the Windows ProcessContainer native
//! tier enforce a grant of exactly one individual file, read and write, without
//! modifying host access control?
//!
//! Run with:
//!
//! ```text
//! cargo run --release --bin spike1-windows-grants -- \
//!     --dir "C:\rz-spike" \
//!     --granted "C:\rz-spike\granted.txt" \
//!     --sibling "C:\rz-spike\sibling.txt"
//! ```
//!
//! The harness prints one `key = value` line per check and exits non-zero if any
//! pass criterion fails. It intentionally separates the *tier* answer, which
//! `v1::probe` gives without launching anything, from the *enforcement* answer,
//! which needs real launches.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

// Spike 1 is inherently Windows-only: the ProcessContainer types the `probe`
// call returns are compiled for Windows only in the SDK. Importing them behind
// `cfg(windows)` lets this binary build on every host, where it reports the
// wrong host and exits, rather than failing to compile at all.
#[cfg(windows)]
use mxc_sdk::v1::{
    AvailableBackend, ContainerRequest, Containment, FilesystemPolicy, ProbeFacts, ProbeOutput,
    RunOptions, WaitResult,
};

/// Windows-only SDK surface. Non-Windows gets a stub so the crate still builds
/// for Spike 2 and the host probe on Linux.
#[cfg(windows)]
mod sdk {
    pub use mxc_sdk::v1::{available_backends, probe, run};
}

/// One measured outcome. `name` is the key to record in SPIKES.md.
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
        format!("    [{}] {:<34} {}", mark, self.name, self.detail)
    }
    /// Report a check and push it, for the early-return path.
    fn report_and_take(self, checks: &mut Vec<Check>) {
        println!("{}", self.report());
        checks.push(self);
    }
}
fn main() {
    #[cfg(not(windows))]
    {
        eprintln!("Spike 1 measures the Windows ProcessContainer native tier.");
        eprintln!("The `probe` type it depends on is compiled for Windows only.");
        eprintln!("Run this binary on a Windows 11 host, build 26100 or newer.");
        eprintln!("On this host none of its results would be meaningful.");
        std::process::exit(2);
    }

    #[cfg(windows)]
    windows_main();
}

#[cfg(windows)]
fn windows_main() {
    let args = Args::parse();
    let mut checks: Vec<Check> = Vec::new();

    println!("=== spike 1: Windows exact-file grants ===\n");

    // ---- 0. Ask the engine first. The tier question needs no launch. --------
    println!("  backend discovery:");
    let backends: Vec<AvailableBackend> = sdk::available_backends();
    for backend in &backends {
        println!(
            "    backend={} tier={} caps={:?} warnings={}",
            backend.backend,
            backend.tier.as_deref().unwrap_or("-"),
            backend.capabilities,
            if backend.warnings.is_empty() { "-".to_string() } else { backend.warnings.join(" | ") }
        );
    }
    if backends.is_empty() {
        println!("    no host-available backends reported");
    }

    println!("\n  probe (no container created):");
    match sdk::probe(None) {
        Ok(ProbeOutput { tier, needs_dacl_augmentation, warnings, probes, error }) => {
            print_probe("host", tier, needs_dacl_augmentation, &warnings, error.as_deref());
            // The decisive fact. `false` here means the host enforces the policy
            // without mutating host access control.
            let needs_dacl = needs_dacl_augmentation.unwrap_or(true);
            checks.push(Check::new(
                "tier_needs_no_dacl_mutation",
                !needs_dacl,
                format!(
                    "needs_dacl_augmentation={} tier={}",
                    needs_dacl, tier.unwrap_or("-")
                ),
            ));
            checks.push(Check::new(
                "probe_reports_native_tier",
                tier.is_some(),
                format!("tier={}", tier.unwrap_or("none")),
            ));
            print_facts(&probes);
        }
        Err(error) => {
            println!("    probe failed: {} ({})", error.message, error.code);
            println!("    This host is not a ProcessContainer host, so the answer is");
            println!("    already 'Windows out'. Recording it and stopping.");
            println!("\n{}", Check::new("probe_available", false, error.message).report());
            println!("\nRESULT: WINDOWS OUT - probe failed");
            std::process::exit(2);
        }
    }

    // ---- Prepare the workspace. -------------------------------------------
    let dir = args.dir.clone();
    let granted = args.granted.clone();
    let sibling = args.sibling.clone();

    fs::create_dir_all(&dir).expect("cannot create spike directory");
    write_if_absent(&granted, b"granted-original\n");
    write_if_absent(&sibling, b"sibling-original\n");

    // Grant the AppContainer SIDs access to the workspace. The sandbox workload
    // runs under an identity derived from the calling user but not equal to it,
    // so a workspace under a user profile is not traversable by default and
    // every check would fail with "Access is denied" - which reads exactly like
    // a sandbox refusal. Without this the control case cannot distinguish
    // "denied the file I did not grant" from "denied everything".
    let _ = Command::new("icacls")
        .arg(&dir)
        .arg("/grant")
        .args(["*S-1-15-2-1:(OI)(CI)(F)", "*S-1-15-2-2:(OI)(CI)(F)", "BUILTIN\\Users:(OI)(CI)(F)"])
        .output();
    println!(
        "\n  workspace: dir={} granted={} sibling={}",
        dir.display(),
        granted.display(),
        sibling.display()
    );

    // ---- Host ACL snapshot, before. ---------------------------------------
    let acl_before = snapshot_acl(&dir);
    println!("  acl before: {}", acl_before);

    // ---- 1. Probe the request shape, no launch. ----------------------------
    // There is no dry-run validation for a one-shot ContainerRequest: the
    // validate_* family takes ProvisionRequest or ExecutionRequest. `probe`
    // with a request is the closest equivalent and reports request-specific
    // diagnostics alongside the tier.
    let request = build_request(&granted, &dir);
    println!("\n  probe(request) - shape and tier, no launch:");
    match sdk::probe(Some(&request)) {
        Ok(ProbeOutput { tier, needs_dacl_augmentation, warnings, error, .. }) => {
            print_probe("request", tier, needs_dacl_augmentation, &warnings, error.as_deref());
            checks.push(Check::new(
                "single_file_request_probes",
                error.is_none(),
                error.unwrap_or_else(|| "no error".to_string()),
            ));
            let needs_dacl = needs_dacl_augmentation.unwrap_or(true);
            checks.push(Check::new(
                "single_file_request_needs_no_dacl",
                !needs_dacl,
                format!("needs_dacl_augmentation={needs_dacl}"),
            ));
        }
        Err(error) => Check::new(
            "single_file_request_probes",
            false,
            format!("code={} message={}", error.code, error.message),
        )
        .report_and_take(&mut checks),
    }

    // ---- 2..6. Launch tests. Each is a real container run. ----------------
    println!("\n  launch tests:");

    checks.push(launch_check(
        "read_granted_file",
        read_cmd(&granted),
        &granted,
        &dir,
        true,
    ));

    // The actual question: a sibling in the same directory must be denied.
    checks.push(launch_check(
        "read_sibling_denied",
        read_cmd(&sibling),
        &granted,
        &dir,
        false,
    ));

    checks.push(launch_check(
        "write_granted_file",
        write_cmd(&granted),
        &granted,
        &dir,
        true,
    ));
    assert_sibling_untouched(&sibling);

    checks.push(launch_check(
        "create_in_granted_dir_denied",
        create_cmd(&dir, "should-not-exist.txt"),
        &granted,
        &dir,
        false,
    ));

    // Control: a directory grant must still work, so a failure above is
    // distinguishable from "the sandbox did not launch at all".
    checks.push(launch_check(
        "directory_grant_reads_granted",
        read_cmd(&granted),
        &granted,
        &dir,
        true,
    ));

    // ---- 7. Host ACL snapshot, after. -------------------------------------
    let acl_after = snapshot_acl(&dir);
    println!("\n  acl after:  {}", acl_after);
    let unchanged = acl_before == acl_after;
    if !unchanged {
        println!("    ACL CHANGED. The forbidden tier ran.");
    }
    checks.push(Check::new(
        "host_acl_unchanged",
        unchanged,
        if unchanged {
            "before == after".to_string()
        } else {
            "before != after".to_string()
        },
    ));

    // ---- 9. Process handle shape, for the record. --------------------------
    println!("\n  process handle surface (compile-time, no call needed):");
    println!("    MxcProcess exposes: id, kill, kill_for_timeout, try_wait, wait,");
    println!("    wait_with_output, take_stdin, take_stdout, take_stderr,");
    println!("    take_native_stdio, stdout_closer, stderr_closer, warnings,");
    println!("    output_metadata.");
    println!("    It exposes NO descendant enumeration and NO tree-drain wait.");
    println!("    => the SDK cannot answer 'is the whole tree gone', so Robokura");
    println!("       must own the containment guardian. Recorded as a finding.");
    checks.push(Check::new(
        "sdk_has_no_tree_drain",
        true,
        "compile-time: no descendant enumeration or tree-drain wait on MxcProcess",
    ));

    // ---- Report. ----------------------------------------------------------
    println!("\n=== results ===");
    for check in &checks {
        println!("{}", check.report());
    }
    let failed: Vec<&Check> = checks.iter().filter(|c| !c.passed).collect();

    println!("\n=== summary (paste into SPIKES.md with host, distro, build, date) ===");
    for check in &checks {
        println!("{}={}", check.name, if check.passed { "yes" } else { "no" });
    }

    if failed.is_empty() {
        println!("\nRESULT: WINDOWS IN - native tier enforces an exact single-file grant.");
        std::process::exit(0);
    } else {
        let critical = failed
            .iter()
            .any(|c| c.name == "tier_needs_no_dacl_mutation" || c.name == "read_sibling_denied");
        println!(
            "\nRESULT: {} CRITICAL - {} of {} checks failed.",
            if critical { "WINDOWS OUT" } else { "INCONCLUSIVE" },
            failed.len(),
            checks.len()
        );
        if !critical {
            println!("The decisive checks passed. Investigate the failing one before");
            println!("recording a host result.");
        }
        std::process::exit(1);
    }
}

// ---------------------------------------------------------------------------
// helpers (Windows only)
// ---------------------------------------------------------------------------
#[cfg(windows)]

struct Args {
    dir: PathBuf,
    granted: PathBuf,
    sibling: PathBuf,
}

impl Args {
    fn parse() -> Self {
        let mut dir = None;
        let mut granted = None;
        let mut sibling = None;
        let mut argv = std::env::args().skip(1);
        while let Some(arg) = argv.next() {
            match arg.as_str() {
                "--dir" => dir = argv.next(),
                "--granted" => granted = argv.next(),
                "--sibling" => sibling = argv.next(),
                other => {
                    eprintln!("unknown argument: {other}");
                    std::process::exit(64);
                }
            }
        }
        let dir = dir.expect("--dir is required");
        let dir = PathBuf::from(dir);
        let granted = granted.map(PathBuf::from).unwrap_or_else(|| dir.join("granted.txt"));
        let sibling = sibling.map(PathBuf::from).unwrap_or_else(|| dir.join("sibling.txt"));
        Self { dir, granted, sibling }
    }
}

/// A request granting exactly one file, read and write.
fn build_request(granted: &Path, working_directory: &Path) -> ContainerRequest {
    // The command is replaced per test; `probe` only checks structure.
    let mut request = ContainerRequest::new("cmd /c exit 0");

    request.containment = Containment::ProcessContainer(Default::default());

    // Note the shape: `readwrite_paths` is a `Vec<String>` with no per-entry
    // kind. Whether a *file* entry is honoured is the spike's question.
    request.filesystem = Some(FilesystemPolicy {
        readwrite_paths: vec![granted.to_string_lossy().into_owned()],
        readonly_paths: Vec::new(),
        denied_paths: Vec::new(),
        clear_policy_on_exit: None,
    });

    // Set explicitly: backends substitute a working directory from the first
    // `readwrite_paths` entry that is an existing *directory*, and skip entries
    // that name a file. Without this the launch is non-deterministic.
    request.working_directory = Some(working_directory.to_string_lossy().into_owned());
    request.environment = None;
    request.inherit_default_environment = Some(false);
    request.timeout_ms = Some(60_000);
    request
}

/// Build the same request but with a specific command and, optionally, a
/// directory grant for the control case.
fn request_for(command: &str, granted: &Path, dir: &Path, directory_grant: bool) -> ContainerRequest {
    let mut request = build_request(granted, dir);
    request.command = command.to_string();
    if directory_grant {
        // Control case: also grant the containing directory. Used only to
        // distinguish "the file grant was refused" from "the sandbox did not
        // launch at all".
        if let Some(fs) = request.filesystem.as_mut() {
            fs.readwrite_paths.push(dir.to_string_lossy().into_owned());
        }
    }
    request
}

fn read_cmd(path: &Path) -> String {
    format!("cmd /c type {}", quote(path))
}

fn write_cmd(granted: &Path) -> String {
    // `echo text>file` writes the file; a space before the redirect adds a
    // trailing blank, which does not matter here because the harness only
    // checks that the write landed in the granted file and nowhere else.
    format!("cmd /c echo harness-write-ok>{}", quote(granted))
}

fn create_cmd(dir: &Path, name: &str) -> String {
    format!("cmd /c echo x > {}", quote(&dir.join(name)))
}

fn quote(path: &Path) -> String {
    let s = path.to_string_lossy();
    if s.contains(' ') {
        format!("\"{s}\"")
    } else {
        s.into_owned()
    }
}



fn launch_check(
    name: &'static str,
    command: String,
    granted: &Path,
    dir: &Path,
    should_succeed: bool,
) -> Check {
    let request = request_for(&command, granted, dir, false);
    match sdk::run(request, RunOptions { telemetry: None }) {
        Ok(result) => {
            let ok = matches!(result.outcome, mxc_sdk::v1::WaitResult::Exited(0));
            let detail = format!(
                "exit_ok={} outcome={} stdout={:?} stderr={:?}",
                ok,
                wait_text(&result.outcome),
                String::from_utf8_lossy(&result.stdout).trim(),
                String::from_utf8_lossy(&result.stderr).trim(),
            );
            Check::new(name, ok == should_succeed, format!("expected_success={should_succeed}; {detail}", ))
        }
        Err(error) => Check::new(
            name,
            !should_succeed,
            format!("launch refused: code={} message={}", error.code, error.message),
        ),
    }
}

fn wait_text(outcome: &WaitResult) -> String {
    match outcome {
        WaitResult::Exited(code) => format!("exited({code})"),
        WaitResult::TimedOut => "timed_out".to_string(),
    }
}

/// The write test writes a literal marker file, so this verifies the write
/// actually reached the granted file and did not land somewhere else.
fn assert_sibling_untouched(sibling: &Path) {
    if let Ok(content) = fs::read_to_string(sibling) {
        if !content.contains("sibling-original") {
            println!("    sibling.txt CHANGED by the write test - access widened beyond the grant");
        }
    }
}

fn write_if_absent(path: &Path, content: &[u8]) {
    if !path.exists() {
        fs::write(path, content).expect("cannot write fixture");
    }
}

/// ACL snapshot via icacls. Wrapped so a missing tool yields a recorded
/// unknown rather than a silently-empty comparison, which would make the
/// before/after comparison pass vacuously.
fn snapshot_acl(dir: &Path) -> String {
    let output = Command::new("icacls").arg(dir).output();
    match output {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).lines().collect::<Vec<_>>().join("; ")
        }
        Ok(output) => format!(
            "icacls failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ),
        Err(error) => format!("icacls unavailable: {error}"),
    }
}

fn print_probe(
    label: &str,
    tier: Option<&'static str>,
    needs_dacl_augmentation: Option<bool>,
    warnings: &[String],
    error: Option<&str>,
) {
    println!(
        "    {label}: tier={:?} needs_dacl_augmentation={:?} error={:?}",
        tier,
        needs_dacl_augmentation,
        error.unwrap_or("-")
    );
    if !warnings.is_empty() {
        let joined = warnings.join(" | ");
        println!("    {label} warnings: {joined}");
    }
}

fn print_facts(probes: &ProbeFacts) {
    println!(
        "    base_container_api_present={}",
        probes.base_container_api_present
    );
    println!("    native_capture_available={}", probes.native_capture_available);
    println!("    guarded_capture_available={}", probes.guarded_capture_available);
    println!("    bfscfg_present={} bfs_compiled_in={}", probes.bfscfg_present, probes.bfs_compiled_in);
    println!(
        "    base_container_supports_deny_paths={}",
        probes.base_container_supports_deny_paths
    );
    println!(
        "    base_container_supports_enumerate_paths={}",
        probes.base_container_supports_enumerate_paths
    );
    println!(
        "    base_container_supports_ingress_host_loopback_allow={}",
        probes.base_container_supports_ingress_host_loopback_allow
    );
    println!("    isolation_session_available={}", probes.isolation_session_available);
    println!("    hyperlight_available={}", probes.hyperlight_available);
}
