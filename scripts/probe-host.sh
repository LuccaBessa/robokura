#!/usr/bin/env bash
# robokura host probe
#
# Measures the host facts that Spike 2 and Spike 3 depend on. Answers no design
# question by itself; it produces the observations to record in SPIKES.md.
#
# Run on a real, unmodified installation of the host you intend to support —
# a full VM is fine, a container is not. Requires sudo for the delegation test.
#
#   ./probe-host.sh            # full probe
#   ./probe-host.sh --no-sudo  # skip the systemd delegation test

set -u

PASS='\033[32mPASS\033[0m'
FAIL='\033[31mFAIL\033[0m'
WARN='\033[33mWARN\033[0m'
INFO='\033[36m..\033[0m'
SKIP='\033[90mskip\033[0m'

USE_SUDO=1
[ "${1:-}" = "--no-sudo" ] && USE_SUDO=0

# Collected results, formatted for SPIKES.md at the end.
RESULTS=()
record() { RESULTS+=("$1"); printf '  %-38s %s\n' "$1"; }

hr() { printf '\n\033[1m%s\033[0m\n' "$1"; }
note() { printf '     %s\n' "$1"; }

# ---------------------------------------------------------------- header ----
hr "Robokura host probe"
note "date: $(date -u +%Y-%m-%d)"
note "host: $(hostname)"
note "kernel: $(uname -r)"
note "distro: $(. /etc/os-release 2>/dev/null && echo "$PRETTY_NAME")"

# ============================================================================
hr "Section 1 - systemd floor (Spike 3a)"
# ============================================================================

SYSTEMD_VERSION="$(systemctl --version 2>/dev/null | head -1 | awk '{print $2}')"
if [ -z "$SYSTEMD_VERSION" ]; then
  record "systemd=MISSING"
  note "no systemctl. This is not a shippable Linux host."
else
  record "systemd=${SYSTEMD_VERSION}"
  MAJOR="${SYSTEMD_VERSION%%.*}"
  if [ "$MAJOR" -ge 250 ]; then
    note "$PASS systemd >= 250; LoadCredentialEncrypted= is available"
  else
    note "$FAIL systemd < 250; encrypted credentials unavailable"
    note "     Check whether this distribution backports the encrypted path"
    note "     before recording a failure - do not assume."
  fi
fi
record "systemd_creds=$(command -v systemd-creds >/dev/null 2>&1 && echo yes || echo no)"

# ============================================================================
hr "Section 2 - cgroup v2 (Spike 2a)"
# ============================================================================

CGROUP_FS_TYPE="$(stat -fc %T /sys/fs/cgroup 2>/dev/null)"
record "cgroup_fstype=${CGROUP_FS_TYPE:-unknown}"
if [ "$CGROUP_FS_TYPE" = "cgroup2fs" ]; then
  note "$PASS unified cgroup v2 - the guardian's drain primitive is usable"
  record "cgroup_controllers=$(tr ' ' '\n' < /sys/fs/cgroup/cgroup.controllers 2>/dev/null | paste -sd, -)"
else
  note "$FAIL not unified cgroup v2. Assume failure until proven otherwise."
  note "     A v1 or hybrid hierarchy does not give cgroup.events populated."
fi

# ============================================================================
hr "Section 3 - unprivileged user namespaces (Spike 2)"
# ============================================================================
# The decisive test is not the sysctl - it is whether a real unprivileged
# process can actually create a namespace. Ubuntu 23.10+ and 24.04 LTS restrict
# this through AppArmor, and the sysctl reads permissive while the launch fails.

unshare -Ur true 2>/dev/null
if [ $? -eq 0 ]; then
  record "userns_granted=yes"
  note "$PASS an unprivileged process can create a user namespace"
else
  record "userns_granted=no"
  note "$FAIL unprivileged userns refused"
  if [ -r /proc/sys/kernel/apparmor_restrict_unprivileged_userns ]; then
    APPARMOR_RESTRICT="$(cat /proc/sys/kernel/apparmor_restrict_unprivileged_userns)"
    record "apparmor_restrict_userns=${APPARMOR_RESTRICT}"
    if [ "$APPARMOR_RESTRICT" = "1" ]; then
      note "     Cause: AppArmor restricts unprivileged userns."
      note "     On Ubuntu 24.04 this is the expected out-of-the-box result."
      note "     Remediation: sysctl -w kernel.apparmor_restrict_unprivileged_userns=0"
      note "     Confirm it persists across reboot before recording the host as ready."
    fi
  fi
fi
record "max_user_namespaces=$(cat /proc/sys/user/max_user_namespaces 2>/dev/null)"

# ============================================================================
hr "Section 4 - sandbox toolchain (Spike 2)"
# ============================================================================

check_binary() {
  local key="$2" path
  path="$(command -v "$1" 2>/dev/null)"
  [ -z "$path" ] && for d in /usr/sbin /sbin /usr/local/sbin; do
    [ -x "$d/$1" ] && path="$d/$1" && break
  done
  if [ -z "$path" ]; then
    record "${key:-$1}=missing"
    note "$FAIL $1 not installed"
  else
    record "${key:-$1}=$path"
    note "$PASS $1 -> $path"
  fi
}

check_binary bwrap bwrap_path
BRWRAP_VER="$(bwrap --version 2>/dev/null | grep -oE '[0-9]+\.[0-9]+(\.[0-9]+)?' | head -1)"
if [ -n "$BRWRAP_VER" ]; then
  record "bwrap_version=$BRWRAP_VER"
  # 0.5.0 is the floor: --ro-bind-try and --clearenv
  if [ "$(printf '%s\n' "0.5.0" "$BRWRAP_VER" | sort -V | head -1)" = "0.5.0" ]; then
    note "$PASS bwrap >= 0.5.0"
  else
    note "$FAIL bwrap < 0.5.0; deny-by-default baseline uses 0.5.0+ flags"
  fi
fi

check_binary slirp4netns
check_binary nsenter
check_binary unshare unshare_path
check_binary iptables iptables_path
check_binary ip6tables
check_binary iptables-restore
check_binary ip6tables-restore
check_binary sh

# iptables must resolve to the nf_tables backend. The legacy backend opens
# /run/xtables.lock before touching any table, and an unprivileged supervisor
# on a root-owned /run cannot take it. This fails at the first rule, not at
# startup, so it is easy to misdiagnose.
IPT_BACKEND=""
if [ -x /usr/sbin/iptables-nft ] && [ "$(readlink -f "$(command -v iptables)")" = "/usr/sbin/iptables-nft" ]; then
  IPT_BACKEND="nf_tables"
elif [ -x /usr/sbin/iptables-legacy ] && [ "$(readlink -f "$(command -v iptables)")" = "/usr/sbin/iptables-legacy" ]; then
  IPT_BACKEND="legacy"
fi
record "iptables_backend=${IPT_BACKEND:-unknown}"
if [ "$IPT_BACKEND" = "legacy" ]; then
  note "$FAIL iptables is legacy; the unprivileged supervisor cannot take"
  note "     /run/xtables.lock on a root-owned /run. Switch to nft."
elif [ "$IPT_BACKEND" = "nf_tables" ]; then
  note "$PASS iptables on nf_tables"
fi

# util-linux unshare needs --map-current-user and --keep-caps for the
# namespace-local firewall mode.
if unshare --help 2>&1 | grep -q -- '--map-current-user'; then
  record "unshare_map_current_user=yes"
else
  record "unshare_map_current_user=no"
  note "$FAIL util-linux unshare lacks --map-current-user"
fi

# nf_conntrack must already be loaded: unprivileged Bubblewrap cannot modprobe.
if grep -q '^nf_conntrack ' /proc/modules 2>/dev/null || [ -d /sys/module/nf_conntrack ]; then
  record "nf_conntrack=loaded"
  note "$PASS nf_conntrack already loaded"
else
  record "nf_conntrack=not_loaded"
  note "$FAIL nf_conntrack not loaded. Unprivileged bwrap cannot modprobe it."
  note "     Remediation: modprobe nf_conntrack, then make it persistent."
fi

# A real bubblewrap launch, not a tool-presence check. This is the probe that
# catches the hosts where every package is installed and the launch still fails.
if bwrap --ro-bind / / --unshare-user --unshare-pid --dev /dev true 2>/dev/null; then
  record "bwrap_launch=ok"
  note "$PASS bwrap launched a real sandbox"
else
  record "bwrap_launch=failed"
  note "$FAIL bwrap installed but could not launch. Tool presence is not evidence."
fi

# ============================================================================
hr "Section 5 - secret service (Spike 3, Linux desktop)"
# ============================================================================

SECRET_OK=no
if command -v busctl >/dev/null 2>&1; then
  busctl --user list 2>/dev/null | grep -qi 'org.freedesktop.secrets' && SECRET_OK=yes
fi
if [ "$SECRET_OK" = no ] && command -v dbus-send >/dev/null 2>&1; then
  dbus-send --session --dest=org.freedesktop.DBus --type=method_call \
    --print-reply /org/freedesktop/DBus org.freedesktop.DBus.ListNames 2>/dev/null \
    | grep -qi 'org.freedesktop.secrets' && SECRET_OK=yes
fi
record "secret_service=$SECRET_OK"
if [ "$SECRET_OK" = yes ]; then
  note "$PASS a secret service is registered on the session bus"
else
  note "$FAIL no secret service recorded. On a desktop host this is a fail-closed"
  note "     condition: agent credentials are refused rather than stored plainly."
  note "     A headless Linux desktop is management and inspection only."
fi

# ============================================================================
hr "Section 6 - TPM (Spike 3b)"
# ============================================================================

if [ -e /dev/tpm0 ] || [ -e /dev/tpmrm0 ]; then
  record "tpm=present"
  note "$PASS a TPM device is visible"
else
  record "tpm=absent"
  note "     No TPM device. This is the common case; it drives the host-key-only"
  note "     decision in Spike 3 part (c), not a failure by itself."
fi
record "tpm_version=$(cat /sys/class/tpm/tpm0/version 2>/dev/null || echo unknown)"

# ============================================================================
hr "Section 7 - cgroup delegation (Spike 2a)"
# ============================================================================

if [ "$USE_SUDO" -eq 0 ]; then
  note "$SKIP skipped with --no-sudo. This is the decisive test; run it."
else
  PROBE_USER="robokura-probe"
  PROBE_UNIT="robokura-probe.service"
  PROBE_SCRIPT="/usr/local/bin/robokura-delegation-probe.sh"

  # Clean any previous run's artefacts before starting.
  sudo systemctl stop "$PROBE_UNIT" 2>/dev/null
  sudo systemctl disable "$PROBE_UNIT" >/dev/null 2>&1
  sudo rm -f "/etc/systemd/system/$PROBE_UNIT" "$PROBE_SCRIPT"

  sudo useradd -M -N -s /usr/sbin/nologin "$PROBE_USER" 2>/dev/null
  sudo tee "$PROBE_SCRIPT" >/dev/null <<'PROBE'
#!/usr/bin/env bash
# Runs inside the service, as an unprivileged user. Reports whether this process
# can create and populate a child cgroup. Writes to a file as well as stdout,
# because a host is not guaranteed to have a running journal.
set -u
OUT=/tmp/rz-probe.out
CG="/sys/fs/cgroup$(awk -F: '$1==0 {print $3}' /proc/self/cgroup)"
{
  echo "uid=$(id -u)"
  echo "service_cgroup=$CG"
  if [ ! -d "$CG" ]; then echo "FAIL: cannot resolve own cgroup"; exit 1; fi
  [ -w "$CG" ] && echo "writable=yes" || echo "writable=no"
  mkdir "$CG/rz-probe" 2>/dev/null && echo "mkdir=yes" || echo "mkdir=no"
  if [ -d "$CG/rz-probe" ]; then
    echo $$ > "$CG/rz-probe/cgroup.procs" 2>/dev/null && echo "procs=yes" || echo "procs=no"
    cat "$CG/rz-probe/cgroup.events" 2>/dev/null | tr '\n' ' '; echo
    [ -w "$CG/rz-probe/cgroup.kill" ] && echo "kill=yes" || echo "kill=no"
  fi
} | tee "$OUT"
PROBE
  sudo chmod 0755 "$PROBE_SCRIPT"

  sudo tee "/etc/systemd/system/$PROBE_UNIT" >/dev/null <<UNIT
[Unit]
Description=Robokura cgroup delegation probe
After=multi-user.target

[Service]
Type=oneshot
User=$PROBE_USER
Delegate=yes
ExecStart=$PROBE_SCRIPT

[Install]
WantedBy=multi-user.target
UNIT

  sudo systemctl daemon-reload
  note ""
  note "Delegated test (the real case: server installed as a service):"
  sudo rm -f /tmp/rz-probe.out
  sudo systemctl start "$PROBE_UNIT" 2>&1 | sed 's/^/     systemd: /'
  sleep 2
  if [ -f /tmp/rz-probe.out ]; then
    cat /tmp/rz-probe.out | grep -E '^(uid|service_cgroup|writable|mkdir|procs|kill|FAIL)' | sed 's/^/     /'
    SVC_OUT="$(cat /tmp/rz-probe.out)"
  else
    # No file: fall back to the journal for hosts that have one.
    note "     probe wrote no output file; trying journal"
    SVC_OUT="$(sudo journalctl -u "$PROBE_UNIT" -o cat --no-pager 2>/dev/null | grep -E '^(service_cgroup|writable|mkdir|procs|kill|FAIL)')"
    printf '%s\n' "$SVC_OUT" | sed 's/^/     /'
  fi

  SVC_DELEG=no
  echo "$SVC_OUT" | grep -q 'mkdir=yes' && echo "$SVC_OUT" | grep -q 'procs=yes' && SVC_DELEG=yes
  record "delegation_service=$SVC_DELEG"
  if [ "$SVC_DELEG" = yes ]; then
    note "$PASS the delegated model works. Linux can be service-installed."
  else
    note "$FAIL the service could not create and populate a delegated child."
    note "     With no delegated subtree the guardian has no drain primitive, so"
    note "     an undelegated host cannot execute bots."
  fi

  note ""
  note "Negative control (a hand-run server, no service unit):"
  # setpriv wants a numeric gid; runuser and sudo -u do not. Try them in order.
  NEG_OUT=""
  if command -v runuser >/dev/null 2>&1; then
    NEG_OUT="$(sudo runuser -u "$PROBE_USER" -- "$PROBE_SCRIPT" 2>&1)"
  elif command -v sudo >/dev/null 2>&1; then
    NEG_OUT="$(sudo -u "$PROBE_USER" "$PROBE_SCRIPT" 2>&1)"
  else
    NEG_OUT="$(sudo setpriv --reuid "$(id -u "$PROBE_USER")" --regid "$(id -g "$PROBE_USER")" \
               --clear-groups "$PROBE_SCRIPT" 2>&1)"
  fi
  sudo rm -f /tmp/rz-probe.out
  NEG_OUT_FILE=""
  [ -f /tmp/rz-probe.out ] && NEG_OUT_FILE="$(sudo cat /tmp/rz-probe.out 2>/dev/null)"
  printf '%s\n' "$NEG_OUT" "$NEG_OUT_FILE" | grep -E '^(uid|service_cgroup|writable|mkdir|procs|kill|FAIL)' | sed 's/^/     /'

  NEG_DELEG=no
  { echo "$NEG_OUT"; echo "$NEG_OUT_FILE"; } | grep -q 'mkdir=yes' && \
    { echo "$NEG_OUT"; echo "$NEG_OUT_FILE"; } | grep -q 'procs=yes' && NEG_DELEG=yes
  record "delegation_hand_run=$NEG_DELEG"
  if [ "$NEG_DELEG" = yes ]; then
    note "$WARN a hand-run server CAN get delegation. This contradicts the"
    note "     architecture's claim that Linux must be service-installed, and"
    note "     would simplify the Linux desktop install flow. Record it."
  else
    note "$PASS a hand-run server cannot delegate. Confirms the documented model:"
    note "     on Linux, bot execution requires the installer-provided service."
  fi

  sudo systemctl disable "$PROBE_UNIT" >/dev/null 2>&1
  sudo rm -f "/etc/systemd/system/$PROBE_UNIT" "$PROBE_SCRIPT"
  sudo rm -f /tmp/rz-probe.out
  sudo systemctl daemon-reload
  sudo userdel "$PROBE_USER" 2>/dev/null
fi

# ============================================================================
hr "Summary - paste into SPIKES.md along with the host, distro and date"
# ============================================================================

printf '\n'
for r in "${RESULTS[@]}"; do echo "$r"; done
printf '\n'

hr "Interpretation"
note "A host is a candidate Linux host only if ALL of these hold:"
note "  systemd >= 250  ·  cgroup_fstype=cgroup2fs  ·  userns_granted=yes"
note "  bwrap_launch=ok  ·  iptables_backend=nf_tables  ·  nf_conntrack=loaded"
note "  delegation_service=yes  ·  delegation_hand_run=no"
note ""
note "Expected out-of-the-box: Ubuntu 24.04 LTS FAILS at userns_granted because"
note "AppArmor restricts it. That is the documented result, not a host defect."
note "A host failing only on tool installation is a prepared-host candidate; one"
note "failing on userns or delegation needs a policy change, not a package."
