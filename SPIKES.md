# Validation spikes

Three questions gate the first release. They are resolved by running code
against real hosts, not by reasoning, which is why they are separated from the
design documents: `SYSTEM_ARCHITECTURE.md` records what the architecture claims,
and this file records what has to be *measured* before those claims become
release criteria.

Each brief states the question, why it blocks v1, the procedure, the pass/fail
criteria, and what each result changes in the planning documents. A spike is
done when its result is recorded here with the host it was measured on and the
date. An untracked observation is not a result.

Two of these need no code at all. `scripts/probe-host.sh` measures every
host-dependent part of Spike 2 and the floor and credential-store parts of
Spike 3, directly on a fresh VM, in a few minutes. Run it before writing any
Rust: it gives you the candidate host list and the two cheapest, most
consequential answers, with no provider account and no harness.

Order matters. Spike 2 has a result that changes what Spike 3 needs to test, and
Spike 1 has the widest blast radius.

---

## Spike 1 — Windows exact-file grants

**STATUS: measured, host-below-floor, negative result.**

Recorded against the host below, with the caveat in "Reading this result".

```
host=Windows 10 Home Single Language, build 2009
probe tier=base-container
needs_dacl_augmentation=false
native_capture_available=true
base_container_supports_deny_paths=true
base_container_supports_enumerate_paths=false
base_container_supports_ingress_host_loopback_allow=false
backend discovery=processcontainer caps=[CaptureDenials, FilesystemDeniedPaths]
read_granted_file=yes
read_sibling_denied=no      <-- the sibling in the same directory WAS readable
write_granted_file=yes
create_in_granted_dir_denied=no
directory_grant_reads_granted=yes   (control case, same behaviour)
host_acl_unchanged=yes
```

**What this means.** The native `base-container` tier does not enforce a grant of
one file. A single-file `readwrite_paths` entry produced access to the sibling
file in the same directory and permitted creating new files there, which is the
exact failure mode this spike exists to detect. The host ACL snapshot was
unchanged before and after, so this is **not** the forbidden DACL-mutation
tier — the native tier genuinely grants the containing directory, consistent
with the documented schema behaviour that a `readwritePaths` entry "applies to
that directory and its descendants" and the fact that the field is a bare
`Vec<String>` with no per-entry kind to distinguish a file from a directory.

**Reading this result.** Two things are true at once and both must be recorded:

- **The negative is real on the host measured.** The control case failed
  identically, which rules out "the sandbox did not launch": the sandbox
  launched, read the granted file, and also read the sibling.
- **The host is below the documented OS floor.** The backend floor is Windows 11
  24H2 (build 26100) and this is build 2009, yet `probe` reported
  `base-container` with native capture available. Two consequences. First, this
  result is evidence, not a release decision — Spike 1's own criteria say a
  negative on an unsupported host must be confirmed on a supported one before
  Windows is declared out. Second, and independently important, **the probe
  reports a tier on a host below the documented floor**, which qualifies the
  backend-floor statement: the floor describes where MXC claims support, not
  where the probe is willing to answer.

A second finding, useful and cheap: the SDK's own capability bits report
`base_container_supports_enumerate_paths=false` on this host. The architecture
already records that `filesystem.enumeratePaths` needs BaseContainer with PSEC
1.1 `fs_enumerate`; this is a live example of that capability simply being
absent.

**To close this spike:** rerun `spike1-windows-grants` on a Windows 11 24H2+
host (build 26100 or newer). A *pass* there resolves the question favourably. A
*fail* there, on a supported host, is the release decision: Windows desktop is
out of v1.

### Question

Can the Windows ProcessContainer **native tier** enforce a grant of exactly one
individual file, read and write, **without modifying host access control**?

### Why it blocks v1

`SYSTEM_ARCHITECTURE.md` specifies exact-file and directory-tree grants as a
capability the sandbox must prove. The published schema describes
`readwritePaths` as a bare `string[]` and documents a grant as applying to "that
directory and its descendants", so whether a *file* entry is honoured is
undocumented. There is no alternative: AppContainer capabilities are a closed
list, code-integrity and application-control engines are signing and policy
systems rather than per-file grant mechanisms, and controlled-folder access is a
block list. A negative answer removes Windows from v1 entirely.

### Before you start

- Windows 11 host. Start with the newest build you can get. The backend floor is
  24H2 (26100), but whether the native tier needs newer than that is itself
  unverified.
- `mxc-sdk` pinned exactly to `1.0.0`, behind the same trait `robokura-sandbox`
  will use. Not a fork, not a different version.
- A test directory containing at least two sibling files, so "granted A, denied
  B" is observable.

### The shape of the answer

The Rust SDK answers this spike's tier question **without launching anything**.
`v1::probe(Option<&ContainerRequest>)` is Windows-only and returns a `ProbeOutput`:

```rust
pub struct ProbeOutput {
  pub tier: Option<&'static str>,
  pub needs_dacl_augmentation: Option<bool>,
  pub warnings: Vec<String>,
  pub probes: ProbeFacts,
  pub error: Option<String>,
}
```

`needs_dacl_augmentation` is the decisive field: when it is `false` the host
enforces the requested policy without host access-control mutation, and when it
is `true` the host would fall back to the forbidden tier. `ProbeFacts` adds the
supporting facts, including `base_container_api_present`,
`native_capture_available`, `base_container_supports_deny_paths`, and
`base_container_supports_enumerate_paths`. This means the tier question is a
single function call, and the launch tests below confirm what the tier actually
*enforces* rather than what it advertises.

**A correction to an earlier version of this brief.** The `fallback.allowDaclMutation`
schema field defaults to `true` and is the trap I originally warned about — but
it is a *JSON config* field. The typed Rust V1 API does not expose it at all:
`ProcessContainerConfig` carries `learning_mode`, `capabilities`,
`capture_denials`, `ui`, `filesystem`, and `network`, and no DACL flag. A typed
Rust consumer therefore cannot request the DACL-mutation tier, which is a
stronger guarantee than the JSON path gives. Robokura consumes the typed API, so
the dangerous default does not reach it.

### What the SDK actually offers, verified by compiling

These were established by building `spikes/` against `mxc-sdk = "=1.0.0"`
rather than by reading documentation, and they correct the brief where the
documentation and the crate disagree:

- **There is no dry-run validation for a one-shot `ContainerRequest`.**
  `container::validate_provision` takes a `ProvisionRequest`, and
  `validate_process` takes an existing `ContainerId` plus an `ExecutionRequest`.
  The way to probe a request shape is **`v1::probe(Some(&request))`**, which is
  Windows-only and returns the tier and request-specific diagnostics without
  creating a container. The brief now uses this.
- **`ProbeFacts` in the published crate has eleven fields**, not the twelve the
  published type reference lists: it has
  `base_container_supports_ingress_host_loopback_allow` but **no**
  `base_container_supports_identityless_loopback_proxy`. `BackendCapability` is
  likewise missing `IdentitylessLoopbackProxy` and is marked `#[non_exhaustive]`.
  The type reference is ahead of the crate; trust the crate.
- **The workspace ACL must be granted to the AppContainer SIDs**
  (`*S-1-15-2-1` and `*S-1-15-2-2`) or every check fails with "Access is
  denied", which is indistinguishable from a sandbox refusal. This is a harness
  requirement, not a host finding.

### Procedure

0. **Ask the engine first.** Call `v1::probe(None)` on the host. Record `tier`,
   `needs_dacl_augmentation`, and the whole of `ProbeFacts`. If
   `needs_dacl_augmentation` is `true`, the host is a Tier 2/3 host and the answer
   is already "Windows out". Record it and stop, because the remaining steps would
   be measuring the wrong thing.
1. **Validate the shape.** A request with `Containment::ProcessContainer(...)`, a
   single file in `filesystem.readwrite_paths`, and `working_directory` set to an
   explicit directory. Run `v1::container::validate_provision`. Does it validate,
   or is it refused?
2. **Set `working_directory` explicitly.** Backends substitute a working directory
   by taking the first `readwrite_paths` entry that is an existing **directory**,
   and the documented behaviour explicitly skips entries that name a file. If the
   granted path is the file, the substitution skips it and you get the
   system-drive root. Point `working_directory` at a known directory so the launch
   is deterministic.
3. **Read the granted file.** `v1::run` with a command that reads the single
   granted file. Expected: exit 0.
4. **Read a sibling.** Read a different file in the same directory that was not
   granted. Expected: non-zero exit. **This is the actual question.** Granting the
   parent directory is the failure mode that silently widens access.
5. **Write the granted file.** Write to it. Expected: succeeds.
6. **Denial behaviour.** Attempt to create a new file in the same directory.
   Expected: fails.
7. **Host ACL snapshot.** Record the DACL of the granted file and its containing
   directory before and after the run, with `icacls` or `Get-Acl`. Expected:
   identical. Any difference means the forbidden tier ran.
8. **Reparse-point containment.** Create a junction inside the granted file's
   directory pointing at a directory outside it, and attempt to read through it.
   Expected: fails.
9. **Process-tree cleanup.** Kill the leader and confirm the complete tree exits.
   `MxcProcess` exposes `id`, `kill`, `try_wait`, `wait`, and `wait_with_output`,
   and - verified against the V1 type reference - **no descendant enumeration and
   no tree-drain wait**. That is direct evidence for the architecture claim that
   the SDK process handle cannot answer the revocation question, and that only the
   Windows job-object path gives a trustworthy tree-drain primitive.

### Pass criteria

All of: `v1::probe` reports a non-DACL tier and `needs_dacl_augmentation
is `false`; the single-file policy validates; read and write of the exact
file succeed; a sibling read fails; a sibling write fails; the ACL snapshot
is unchanged; the junction does not escape; the tree drains. The probe
result alone answers the tier question and is recorded even when a later
step fails, because it explains which tier ran.

### Result → document changes

| Result | Change |
| --- | --- |
| **Pass** | Windows desktop stays in v1. The host gate row's "require exact selected-file grants" becomes a tested claim, and the exact-file capability key becomes `available` for `processcontainer`. |
| **Sibling read or write succeeds** | Windows is **out** of v1. The gate cannot be satisfied at any strength; the server is management and remote only. |
| **Policy refused** | Same: out of v1. Record the rejection message. |
| **ACL changed** | Same: out of v1. This is the forbidden tier running despite the flag, so it is a harness bug rather than a host result. Fix the harness and rerun before recording a failure. |

### Effort

Half a day on a Windows 11 box with a current SDK available. The only
prerequisite with lead time is a host on a new enough build.

---

## Spike 2 — cgroup delegation and the containment guardian

### Question

Two questions on the same mechanism, and both must pass:

**(a)** Does a server running under the guided installer get a **writable,
delegated** cgroup v2 subtree, on both a Linux VPS and a Linux desktop?

**(b)** Does a workload that **starts its own session** (`setsid()`) actually
drain, so `cgroup.events` reaches `populated 0`?

### Why it blocks v1

The revocation guarantee rests entirely on this. `cleanup_confirmed_at` is written
only when the guardian observes an empty cgroup, and a kill request is explicitly
not evidence of exit. If either half fails there is no revocation guarantee on
Linux, and there is no fallback: no alternative mechanism catches a
`setsid()`-escaping descendant. Both Linux hosts are v1 hosts, so this gates the
release.

It also answers a second v1 question: whether the Linux server can ever be
app-started. The current assumption is that it cannot, and this spike confirms it
rather than assuming it.

### Before you start

- Each candidate Linux image **and** each candidate desktop distribution, as
  separate lists. Ubuntu 24.04 LTS is expected to fail before the rest of this
  runs, because it restricts unprivileged user namespaces through AppArmor.
- `bwrap` 0.5.0+, `slirp4netns`, `nsenter`, the `iptables` front-end on `nf_tables`,
  and `nf_conntrack` already loaded. Unprivileged Bubblewrap cannot `modprobe`.
- A writable `/run/xtables.lock` for the caller's uid, which a root-owned `/run`
  does not provide to a service user.

### Procedure

**Part (a) — delegation, VPS**

1. On each candidate image: `systemd --version`. Record it; this feeds Spike 3.
2. Confirm unified cgroup v2: `stat -fc %T /sys/fs/cgroup` must report `cgroup2fs`.
3. Install the service unit as the installer will, with delegation requested.
   Confirm the delegated parent exists and is writable **as the service user**,
   not as root. This is the part a root-shell test gets wrong.
4. From the service's context, create a sub-cgroup and write a PID into its
   `cgroup.procs`. Confirm it lands.
5. Launch a workload through the container and confirm its PID appears in the
   slice. If delegation is present but the workload escapes the slice, the
   guardian has no visibility and the candidate fails.

**Part (a) — delegation, desktop, and the negative control**

6. Repeat on each desktop distribution.
7. **Negative control, deliberately.** Run the server as an ordinary
   unprivileged process with no service unit, the "just ran the binary" path.
   Attempt to create a delegated sub-cgroup. Expected: **fails**. This confirms
   the architecture's claim that a hand-started Linux server is
   management-and-inspection only. If it unexpectedly succeeds, the installer is
   optional and the Linux desktop install flow gets simpler. Record it.

**Part (b) — the escaping descendant**

8. Write a workload that starts, calls `setsid()`, spawns a grandchild, then
   exits its own session. Kill the group leader with `SIGKILL`.
9. Watch `cgroup.events` on the slice. **Does `populated` reach 0?**
10. Repeat with the workload unsharing further after `setsid()`. Confirm the
    grandchild stays in the same cgroup. Cgroups are not namespaced, so it
    should, and that is the property the guarantee rests on.
11. **Confirm the failure mode is real.** Put a child into uninterruptible sleep,
    or leave a zombie whose parent never reaps it, and confirm `populated` never
    reaches 0. This is not a rhetorical step: it validates that
    `cleanup_unknown` and the owner force-release path are genuinely necessary
    rather than defensive over-engineering. If every case drains cleanly, the
    force-release path is dead code and should be reconsidered.

### Pass criteria

- **VPS:** the service user obtains a writable delegated subtree on at least one
  shippable image, and the workload's PID lands in it.
- **Desktop:** the same on at least one shippable distribution.
- **Negative control:** a hand-run server cannot obtain it.
- **Guardian:** a `setsid()`-escaping descendant drains to `populated 0` after
  the leader is killed.

### Result → document changes

| Result | Change |
| --- | --- |
| **All pass** | Linux VPS and Linux desktop stay in v1. The revocation guarantee becomes a tested claim, and `sandbox_process_tree_cleanup_unconfirmed` becomes a reachable state rather than a hypothetical. |
| **Delegation unavailable on both** | Both Linux hosts are out of v1. Linux work moves to a later phase, which also removes the `slirp4netns`, `iptables`, and `nf_conntrack` chain and the systemd floor from the v1 critical path. |
| **Works on VPS but not desktop** | Linux desktop is out; VPS stays. The installer requirement applies only to VPS. |
| **Negative control succeeds** | The app-started model may be viable on Linux after all. The "Linux server must be service-installed" claim is wrong and should be removed. This would be a simplification, so it is worth testing early. |
| **Escaping descendant does not drain** | The revocation guarantee is unproven on Linux. Same outcome as delegation failing: Linux out of v1 until a different primitive is found. |

### Effort

One to two days. Most of it is provisioning and installing packages on fresh
images. The actual test is short.

---

## Spike 3 — systemd floor and the credential policy

### Question

**(a)** Which systemd version do the candidate images actually ship?

**(b)** Does a candidate VPS expose a TPM?

**(c)** May a vTPM-less host use a **host-key-only** credential policy, or must
it be refused?

### Why it blocks v1

This is a v1 blocker rather than a refinement because the current text requires a
policy combining the host key with TPM protection, and refuses to store
credentials when neither is present. Read against reality, most VPS providers do
not expose a TPM, so the platform as written works on a minority of hosts.
`SYSTEM_ARCHITECTURE.md` also now uses encrypted `LoadCredentialEncrypted=` for
the Linux bootstrap channel, which tightens rather than loosens the floor.

### Procedure

**Part (a) — the floor**

1. For each candidate VPS image and desktop distribution: `systemd --version`.
   Record the exact version, not "at least 250".
2. For anything below 250, check whether the distribution backports the
   encrypted credential path. Do not assume; measure.
3. Verify `LoadCredentialEncrypted=` works end to end on a 250+ host: install the
   unit, restart the service, read the credential from `$CREDENTIALS_DIRECTORY`.
4. Confirm the bootstrap-specific behaviour: the secret is readable **exactly
   once**. Restart the service with the same credential content and confirm the
   second launch is rejected as a replay.
5. Confirm the credential is **not** readable from the unit file. This is the
   whole reason for the encrypted form; if it leaks, the floor is not the fix.

**Part (b) — TPM presence**

6. For each candidate VPS provider and image: does `/dev/tpm0` or
   `/sys/class/tpm0` exist? Does `systemd-creds` succeed with a TPM policy?
7. This part is a survey, not a test. Check the provider's feature list as well
   as the running machine, since some providers expose a vTPM only on certain
   plans.

**Part (c) — the fallback decision**

8. Test `systemd-creds` with a **host-key-only** policy on a vTPM-less host.
   Confirm it encrypts and decrypts.
9. Test the update-resilience requirement from the plan: apply a kernel update
   and confirm the credential still decrypts. Prefer a measurement policy that
   survives updates over binding exact PCR values.
10. Test the failure the documents already promise: confirm a credential
    encrypted on one host does **not** decrypt on a different host. The
    cross-generation refusal is the reason the backup manifest records the
    credential key policy.

### Pass criteria

- At least one shippable VPS image and one desktop distribution at or above the
  systemd floor.
- `LoadCredentialEncrypted=` works, is not readable in the unit file, and the
  secret is consumed exactly once.
- A host-key-only policy works on a vTPM-less host.
- The credential survives an OS update, and refuses cross-generation restore.

### Result → document changes

| Result | Change |
| --- | --- |
| **Host-key-only works** | Adopt it as the vTPM-less fallback and record the **narrower bind**: it protects against theft of the credential material alone, not against theft of the whole disk image. State that in `TECHNICAL_DECISIONS.md`, because the honest claim is smaller than "encrypted at rest". |
| **Host-key-only refused** | Linux VPS support is restricted to providers exposing a vTPM. That is a product decision, not a technical one. Record which providers qualify. |
| **No image reaches the floor** | Linux is out of v1 for the same reason as Spike 2. Verify the floor claim before reaching this conclusion, because it is easy to misread a backported distribution as being below it. |
| **Credential readable in the unit file** | The encrypted path is not doing what it says. Fix the harness and rerun before recording. |

### Effort

Half a day of testing plus a survey of provider TPM support, which is the part
with lead time because it involves asking providers rather than running code.

---

## Sequencing

```
Spike 1 ──────────────────────────► Windows in / out
Spike 2 ───┬──► Linux in / out
Spike 3 ───┘     (3 depends on 2's image list)
```

- **Spike 2 first.** Its result decides whether Linux ships, and its candidate
  image list is the input to Spike 3.
- **Spike 1 in parallel**, on different hardware. It has the widest blast radius
  and the shortest setup.
- **Spike 3 last**, but its part (a) is cheap and should be captured during
  Spike 2's image provisioning: record `systemd --version` on every image you
  spin up anyway.

Once these three land, the remaining open items are later-feature work: the
notification routes, the published capability matrix and error-code catalogue,
per-agent egress qualification, and the `unqualified` first-run rule.
