# Host probe runbook

Executable companion to `SPIKES.md`. Answers the host-dependent parts of Spike 2
(cgroup delegation and the Linux prerequisites) and Spike 3 part (a) and (c)
(systemd floor, credential store, TPM presence) without needing the Rust
harnesses or a cloud account.

## Usage

Copy to the host under test and run it:

```bash
scp scripts/probe-host.sh you@host:~
ssh you@host 'chmod +x probe-host.sh && ./probe-host.sh'
```

Or, on a Hyper-V VM, paste the file through the console.

**Run as a normal user, not root.** The probe's whole subject is what an
*unprivileged* process can do. It will `sudo` for exactly two things: installing
the temporary probe unit, and removing its own artefacts.

```bash
./probe-host.sh --no-sudo   # skip the delegation test only
```

The delegation test is the decisive one, so `--no-sudo` leaves the most
important question unanswered. Use it only on a host you cannot get sudo on.

## What it does

| Section | Covers |
|---|---|
| 1 | systemd version, `systemd-creds` availability — the Spike 3 floor |
| 2 | cgroup v2 unified hierarchy and available controllers |
| 3 | Whether an unprivileged process can actually create a user namespace, and whether AppArmor is the reason it can't |
| 4 | The full sandbox toolchain: `bwrap` version, `slirp4netns`, `nsenter`, `unshare` flags, iptables backend, `nf_conntrack`, and **a real `bwrap` launch** |
| 5 | Whether a secret service is registered on the session bus |
| 6 | TPM device presence |
| 7 | The delegation test: a service-installed probe **and** a hand-run negative control |

It writes nothing outside `/tmp/rz-probe.out` and a temporary systemd unit, both
removed at the end. It does not modify the host's configuration.

## Reading the output

The script ends with a `key=value` summary and an interpretation block. A host
is a candidate Linux host only if **all** of these hold:

```
systemd >= 250
cgroup_fstype=cgroup2fs
userns_granted=yes
bwrap_launch=ok
iptables_backend=nf_tables
nf_conntrack=loaded
delegation_service=yes
delegation_hand_run=no
```

Two results are worth knowing before you run it:

- **`userns_granted=no` on Ubuntu 24.04 LTS is the expected out-of-the-box
  result.** AppArmor restricts unprivileged user namespaces. That is the
  documented failure, not a host defect, and it is the exact reason the plan
  calls Linux a prepared-host platform.
- **`delegation_hand_run=yes` would contradict the architecture.** The plan
  currently states that Linux must be service-installed because a hand-run
  server cannot get a delegated cgroup. If the negative control disproves that,
  it is a *simplification* — record it and tell the plan it was wrong.

A host that fails only on missing tools is a prepared-host candidate. One that
fails on user namespaces or delegation needs a policy change, not a package.

## What it does not answer

- **Spike 1, entirely.** Windows grants need `mxc-sdk` and a Windows host.
- **Spike 2 part (b).** The `setsid()`-descendant drain test needs a workload
  that escapes its session, which is a Rust harness.
- **Spike 3 part (c).** The host-key-only vs TPM policy decision needs
  `systemd-creds` encryption tests and a provider TPM survey.

Those need the harnesses. This runbook gets you the host list and the two
cheapest, highest-consequence answers first.
