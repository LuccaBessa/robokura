# Technical Decisions

This file records the architecture and technology choices for the new
Robokura product. It is separate from the product-scope plan.

## Confirmed architecture

### Process shape

- The desktop app and Robokura Server run as separate processes.
- The same server program serves local installations and Linux VPS hosts.
- The server owns bots, conversations, persistence, background work, and ACP
  sessions. The app does not own product records or speak ACP directly.
- Integrate agents using stable ACP protocol v1 through the official
  `agent-client-protocol` Rust runtime crate. Keep that SDK behind
  `robokura-acp`; do not depend on unstable protocol features for core
  behavior.
- The app and a separate notification process connect to servers through the
  Robokura client interface.
- The notification process can outlive the app window. It exits when no
  connected server needs it, including when the local server shuts down.
- Each server process uses one Tokio runtime shared by its API, event
  connections, routines, and ACP sessions. Agents do not get separate runtimes.

### Language and desktop framework

- Use Rust across the workspace.
- Build the desktop app with GPUI and GPUI Kit.
- Initial desktop targets are macOS on Intel and Apple Silicon, Linux on x86-64,
  and Windows on x86-64.

### App and server communication

- The app and notification process use the same client library.
- The client sends commands over an HTTP API and receives live updates over a
  WebSocket connection.
- The API is versioned. Events carry sequence IDs so a client can resume after
  reconnecting or reload current state when it has missed events.
- Use versioned JSON for HTTP request, response, and WebSocket event bodies.
  Define explicit Serde types in `robokura-api` and keep them separate from
  internal domain and database records.
- Use resource-oriented HTTP operations for records and explicit action
  operations for guarded lifecycle transitions. Clients cannot patch lifecycle
  states directly.
- Give each event a server sequence, event ID, type, resource reference,
  timestamp, and typed payload. State snapshots include the sequence boundary
  needed to resume the WebSocket stream.
- Give every paired-device JSON command a client-generated command ID. Commit
  the state change, ordered event, and command receipt atomically. Repeating
  the same ID and payload returns the original outcome and resource
  identifiers; reusing an ID with a different payload is rejected. Receipts
  live for the paired
  device's lifetime and contain no request bodies or full resource responses.
- Hash a command's method, normalized path, and canonical JSON body to bind its
  ID to exactly one operation. Malformed and unauthorized requests create no
  receipt; valid deterministic domain outcomes do. External-work receipts mean
  accepted, while the durable operation and events report completion.
- The binary upload-content PUT uses its upload ID, expected length, and
  verified SHA-256 digest for idempotent retries without retaining file bytes
  in command receipts. Pairing exchange and agent-auth input are excluded from
  receipts because they carry credentials.
- Use lowercase canonical UUIDv7 text for Robokura IDs and client command IDs.
  Keep ACP IDs opaque. Use RFC 8785 JCS over validated request JSON and SHA-256
  over a domain-separated method/path/body string for receipt fingerprints.
- Keep sync snapshots bounded: return summaries and a sequence boundary, then
  page conversation items and notification history separately. A cursor older
  than event retention gets a snapshot; a stale WebSocket cursor gets
  `resync_required` and the client syncs again. Streamed message updates revise
  one transcript item and emit coalesced update events, not one item per chunk.
- Use provider idempotency keys for external side effects when available. If a
  provider cannot establish whether an action succeeded after a connection
  failure, mark the result uncertain and ask the owner rather than retrying
  blindly.

### Network stack and runtime ownership

- The server uses Axum on Tokio for its HTTP API and WebSocket event endpoint.
- The client uses `reqwest` for HTTP and `tokio-tungstenite` for WebSockets.
- Each long-running process owns at most one Tokio runtime. The server owns its
  runtime for the API, events, routines, and ACP sessions. The app and notifier
  each use one background Tokio runtime for client networking.
- `robokura-client` exposes network operations but does not create a runtime.
  The app passes results back to GPUI without running network work on the UI
  thread.

### Persistence

- Each server stores its data in a SQLite database.
- The server uses `rusqlite` and keeps the SQLite connection on one dedicated
  database worker thread.
- Server tasks send database requests to that worker and await their results.
  The worker owns migrations and transactions and keeps synchronous database
  work off Tokio's async workers.
- A single worker serializes database operations for the initial single-owner
  server. Add read connections only if measured workloads need them.

### Sandbox architecture

- Define one Robokura sandbox policy and enforce it through an OS-specific
  backend on each supported server host platform.
- Check backend capabilities against each requested policy. Refuse to run when
  the backend cannot enforce the required boundary; never silently weaken it.
- Keep local bot execution as a goal on macOS, Linux, and Windows server hosts.
- Microsoft Execution Container (MXC) is the selected enforcement engine,
  consumed only through `robokura-sandbox`. It is **pinned to an exact version**
  and wrapped behind a trait rather than forked: the Rust SDK reached `1.0.0`
  very recently after consolidating an alpha line, and the project's support
  policy is limited to issue tracking with no service commitment. Any minor bump
  re-runs the validation suite, and a raw-`bubblewrap` backend is kept behind a
  feature flag as a same-week escape hatch for the Linux path.
- Do not rely on the backend's own capability query as the go/no-go decision.
  Its documentation states the probe is advisory and acknowledges fields that
  were declared but not enforced. `robokura-sandbox` runs its own probe suite
  against a **real launched sandbox**, because tool-presence checks pass on
  hosts where an actual launch fails, including images that restrict
  unprivileged user namespaces through mandatory access control rather than a
  sysctl.
- **Robokura owns three things no reviewed backend provides:** a containment
  guardian that owns the whole process tree and is the only writer of
  `cleanup_confirmed_at`; a launch identity stub that re-verifies each granted
  path's filesystem identity from inside the sandbox before executing the
  agent; and the probe suite itself. Per host the guardian uses a cgroup v2
  slice, an audit-token session drain, and a job-object handle respectively.
  The SDK's own process handle exposes no descendant enumeration and no
  tree-drain wait, and its Linux kill path signals a process group whose leader
  is not the sandbox's namespace init, so a workload that starts its own
  session would otherwise escape. A kill request is never evidence of exit.
- macOS has exactly one backend, `seatbelt`, with no fallback in the engine at
  all, and Bubblewrap is not a cross-platform story. Seatbelt requires macOS 15
  or later. A macOS validation failure therefore has nowhere to fall back to and
  means macOS local execution does not ship; only Linux has a same-week
  alternative backend.
- Landlock is a **hardening layer applied by the launch stub before exec**,
  never the isolation boundary. It is monotonic, already-open file descriptors
  fall outside newly applied restrictions, and it cannot produce default-deny by
  itself. What it adds over a mount namespace is scoping abstract unix sockets
  and cross-domain signals, and that requires **ABI 5, Linux 6.12 or later**;
  on older kernels the stub applies what it can, reports the degraded scope with
  `host_kernel_feature_missing`, and Robokura makes no socket or signal claim. It
  does not cover UDP either, so egress prevention is the proxy's job alone.
- Run one sandboxed ACP process per work run; the owner approved this execution
  boundary so temporary grants end with the process. Preserve bot workspaces
  and transcripts; resume ACP sessions only where supported, otherwise
  reconstruct context. Validate agent and backend compatibility before
  implementation.
- Path grants are selected through the connected server's filesystem browser.
  The client sends a short-lived, device-bound selection reference, never an
  arbitrary path as grant authority. Create the message, run, and confirmed
  read/write grants atomically before ACP launch. Browse without following
  symlinks or reparse points, revalidate filesystem identity at launch, and
  refuse a selection if the backend cannot enforce its exact boundary.
- A run's filesystem policy is immutable after process launch. Revoking a path
  transitions the run to `stopping`, terminates the complete contained process
  tree, and reports the grant as revoked only after process exit is confirmed.
  A failed cleanup remains visible and is retried; a stopping run is never
  resumed. After a server restart, a resumable run uses a fresh sandbox and
  revalidates still-active grants. Do not use a backend fallback that changes
  host file ACLs; if the selected backend requires that fallback, refuse to
  start until a separate product decision explicitly approves the behavior.
- Evaluate sandbox capabilities per backend and host, not only by the shared
  policy schema. A backend must prove default-deny isolation, exact file and
  directory-tree scope, and complete process tree termination.
- **Linux prerequisites are a chain, not a pair.** Beyond unprivileged user
  namespaces actually being granted and `bwrap` 0.5.0+, the proxy-only egress
  posture that provider connectivity depends on also requires `slirp4netns`,
  util-linux `unshare` with `--map-current-user` and `--keep-caps`, `nsenter`,
  the `iptables`/`ip6tables` family resolving to the `nf_tables` backend with a
  writable `/run/xtables.lock` for the caller's uid, and the `nf_conntrack`
  module already loaded — unprivileged Bubblewrap cannot `modprobe` it. Add a
  delegated, writable cgroup v2 subtree, without which the containment guardian
  has no drain primitive and the host cannot execute bots at all. Each of these
  fails at a different point in the sequence, which is the concrete reason the
  probe launches a real sandbox instead of checking for tools.
- Set `fallback.allowDaclMutation: false` in **every** compiled Windows policy.
  The schema default is `true`, and that default is precisely the host-DACL
  mutation this design forbids. Setting it explicitly is free and converts a
  silent downgrade into a loud refusal on hosts that need the Tier 3 fallback.
- **Windows is decided by one spike, not by assumption.** The published schema
  describes grants as a directory and its descendants, and the field is a bare
  string list with no entry kind, so exact-file support under the native
  no-DACL-mutation tier is undocumented and unproven. There is no alternative
  that avoids host DACL mutation: AppContainer capabilities are a closed list,
  code-integrity and application-control engines are signing and policy systems
  rather than per-file grant mechanisms, and controlled-folder access is a block
  list. If a single-file grant fails, Windows local bot execution is out of scope
  for the first release and the server is management and remote only on that
  host. The Windows host-preparation tool persists ACEs on the system volume,
  rewrites device security descriptors at every boot, and requires elevation; it
  exists only to serve the forbidden tier, so it is never run.
- macOS uses Seatbelt with direct egress denied and a server-managed loopback
  proxy for provider access; qualify every supported agent/auth flow to verify
  destination enforcement and that direct sockets remain blocked. Because
  Seatbelt cannot scope inbound access to loopback rather than every local
  address, agents must not need to listen, and the proxy binds loopback
  explicitly by convention. Two further Seatbelt behaviours are recorded as
  hard invariants in the capability matrix: granting a read/write path also
  grants socket connect within that subtree, so any agent socket under a
  workspace is explicitly denied; and the option that permits UI access grants
  read and write across shared temporary directories regardless of the
  filesystem policy, so it stays off.
- Windows Sandbox maps directories, cannot isolate an individual selected file
  through its mapped-folder interface, permits one active VM per logon session,
  and documents best-effort teardown. It is never a fallback.
- Egress confinement rests on kernel-level denial with a single loopback
  exception, **not** on an agent honoring proxy environment variables, which the
  engine cannot force on macOS. A client that ignores proxy settings fails to
  reach its provider; it does not reach the internet. Direct-socket blocking is
  therefore a baseline check. The server-managed proxy carries a per-run bearer
  token so another local process cannot borrow the run's egress path, enforces
  its destination allowlist on connect authority, resolves names itself so the
  sandbox needs no resolver, and pins the resolved address for the tunnel's
  lifetime to blunt rebinding.
- Record the **bounded** security claim rather than an absolute one. No reviewed
  backend binds a grant by filesystem object identity; all bind by path at mount
  or profile generation time, so the check-to-launch window cannot be fully
  closed. Because the deny side is authoritative everywhere, the worst case of
  losing that race is access to exactly one unintended object, and that bound is
  documented. On macOS the claim is explicitly that a bot cannot casually reach
  resources outside its workspace and grants and cannot egress beyond the proxy,
  **not** that a hostile model plus its tools cannot escape; `execution_status`
  there reports `limited`.
- ACP session restoration is capability-negotiated. Check `loadSession` before
  `session/load`; load replays conversation history, while `session/resume` is
  separate and also optional. Without an advertised usable restore capability,
  create a new session and provide bounded context from Robokura's durable
  history. Do not duplicate replayed content in the Robokura transcript or
  claim exact continuity for a reconstructed session. In the current agent
  population session loading is close to universal while resumption is not, so
  the two are recorded separately, and a large share of agents require
  authentication before accepting a new session, which means the restore path
  cannot be validated at install time on a remote server: it stays `unverified`
  until the agent first completes an authenticated prompt. Probe and record
  capabilities per installed version; track protocol readiness, restore
  support, and sandbox-lifecycle validation separately, and revalidate after
  agent updates.

### Notification process

- Start one notifier process at user login and let it outlive the app window.
- The notifier subscribes to all configured servers. When one server shuts
  down, it drops that subscription and exits only when no watched servers
  remain.
- The app reuses an existing notifier instead of starting a second one.
- Deliver notifications through native operating-system notification systems
  behind an internal interface. Use `notify-rust`; keep behavior within the
  common cross-platform feature set and validate supported actions per OS.

### Local server discovery and startup

- The local server binds to a configurable, stable loopback address and exposes
  a versioned health endpoint.
- On launch, the app attaches to a compatible server that is already ready. If
  none responds, it starts the separate server executable and waits for
  readiness.
- The server's bind prevents a second server from claiming the same address. If
  the address is occupied by an incompatible server or another process, the
  app explains the conflict and does not start a duplicate.
- The app records whether it started or attached to the server. Closing the app
  leaves the server running. The tray's explicit shutdown action requests a
  graceful server shutdown.
- Local mode binds only to loopback. Remote server lifecycle is independent of
  the app.

### Event replay and recovery

- SQLite remains the source of truth. WebSockets deliver live changes but are
  not the only copy of application state.
- Persist compact, ordered event records for thirty days. Coalesce streamed
  output rather than storing every text chunk as a separate event.
- A reconnecting client provides its last event ID. The server replays later
  events when retained. If the cursor is older than the replay window, the
  server requires a resync from current state and returns the snapshot's
  sequence boundary.
- Keep notification history separately until the owner clears it. The notifier
  can fetch missed notifications after a long disconnect.

### Paired-device credentials

- Pair a remote device using a single-use, 256-bit random code created by the
  server CLI. It expires after ten minutes, is rate-limited, and is accepted
  only over HTTPS. Register a named device and return its unique 256-bit token
  once; store only verifiers server-side. The owner can revoke the device
  immediately. For local first pairing, pass the bootstrap code over a private
  app/server control pipe, not command-line arguments, environment variables,
  or logs.
- Store each device's server token in the client's operating-system credential
  store. The server stores only a verifier for each token so the owner can
  revoke a device without keeping its usable credential.
- Require HTTPS with normal certificate validation for direct remote
  connections and end-to-end inside an optional SSH tunnel.
- The SSH tunnel uses the owner's existing SSH agent and host configuration.
  Robokura does not store SSH passwords or private keys.
- Run the notifier as the same operating-system user as the app so both
  processes can use the same credential store. Do not copy tokens into plain
  configuration files or backups.
- If the operating-system credential store is unavailable, report the problem
  instead of silently saving a token in plaintext.
- Use the Rust `keyring` crate's `v1` API with native operating-system stores:
  macOS Keychain, Windows Credential Manager, and Linux Secret Service. If the
  Linux Secret Service is unavailable, report the problem; do not fall back to
  plaintext storage.

### File and authentication transports

- Pair remotely through a server-side CLI over SSH or local console; the app
  exchanges the 10-minute code only in an HTTPS request body. Keep pairing
  creation off unauthenticated HTTP routes. A loopback peer may exchange a code
  over plain HTTP, which local mode requires; a non-loopback peer must use HTTPS
  with normal certificate validation, and the server verifies at startup that its
  public listener terminates TLS rather than trusting configuration.
- For an app-started local server on macOS or Windows, deliver the bootstrap
  code over an **inherited
  anonymous pipe or socket pair**, never a named pipe, named socket, or file
  path. A named channel cannot be private against same-user processes: the app,
  the notifier, and every other desktop program run as that user, and a Windows
  pipe DACL or a `0700` directory only excludes other users and remote clients.
  Secrecy comes from possessing a handle. On Windows the app passes exactly the
  four pipe handles in a process-thread handle list so the server inherits
  nothing else; on Unix it duplicates the pair into two reserved descriptors and
  clears the close-on-exec flag on only those, which Rust's defaults give for
  free. The server enables its bootstrap path only when that descriptor reports a
  pipe or socket, so a server started from a terminal or a service manager cannot
  be fed a secret by accident and the external-start fallback becomes an
  invariant. Any other app-spawned child, including the notifier, uses a separate
  handle set.
- A second, named local channel serves the tray's graceful shutdown and the CLI
  creating a pairing code against a running server. It requires path selection
  and permissions, and because a same-user process can connect to it, every
  message is mutually authenticated with an HMAC over a per-launch key file. On
  macOS keep the socket path short, because the platform's socket path length is
  limited and the temporary directory already consumes most of it.
- Relay an ACP terminal-auth PTY through an authenticated, session-scoped
  WebSocket. Binary frames carry terminal bytes and JSON frames carry resize
  and lifecycle controls. Do not persist PTY output; terminate the auth attempt
  if the socket closes or the server restarts. This is separate from ACP's
  `terminal/*` methods used by an agent during a bot run.
- Exclude pairing exchanges and agent-auth input from command receipt hashes so
  one-time codes and low-entropy credentials cannot be exposed through stored
  fingerprints. On a lost auth-input response, read session state and never
  resend the secret automatically.
- Transfer attachments through an upload intent, authenticated streamed HTTP
  content PUT, digest verification, and an explicit finalize command. Promote
  verified files to opaque server storage keys. Unattached uploads expire;
  attached files follow conversation retention. Stage attached input read-only
  inside the work-run sandbox.
- **Rename before commit, never the reverse.** Persist the promoting intent with
  its generated identifier and final storage key, move the file, then commit the
  asset record. After a crash the failure mode is an orphan file with no row,
  which a sweep deletes cheaply; the opposite order would let a client learn an
  identifier whose bytes do not exist. Derive the storage key from the
  server-generated identifier so collisions are impossible, which keeps
  rename-replace semantics and antivirus handle races out of the promotion path.
  Assert at startup that staging and the final store share one volume, since a
  cross-device rename fails midway.
- Carry integrity metadata in `Content-Digest`, not the older digest header,
  which has been obsoleted. The value is a structured-fields byte sequence;
  the older `sha256=<hex>` spelling is non-conformant and is rejected. Refuse a
  digest supplied only in a trailer, because a trailer is readable only after
  the body is complete, which defeats verification.

### Connected services

- Store connected-service credentials server-side in a dedicated credential
  store, separate from ordinary product records. Route agent service calls
  through a server-side broker; never expose raw service tokens to ACP agents.
- Exclude connected-service credentials from backups and exports.
- Encrypt credentials at rest using platform-managed keys, without requiring an
  owner-entered master password for each unattended server restart. Use the OS
  credential store on desktop hosts. On Linux desktop that is the **secret
  service** over D-Bus — GNOME Keyring or KWallet — which is not present on every
  machine that runs a desktop, so probe it at install time and **refuse** to store
  agent credentials when it is unavailable, matching the rule that a missing
  secure store is a refusal rather than a plaintext fallback. A headless Linux
  desktop with no session bus is therefore a management and inspection host only.
- For a Linux VPS use systemd encrypted credentials, which requires
  **systemd 250 or later**. Prefer a policy combining the host key with TPM
  protection; the combined default fails outright when there is neither a TPM nor
  a persistent host key, so probe both at install time and **refuse** to store
  agent or service credentials when neither holds, matching the rule that a
  missing secure store is a refusal rather than a plaintext fallback. Prefer
  binding to a measurement policy that survives OS updates over binding to exact
  measurement values, so an update does not render stored credentials
  undecryptable. **Whether a vTPM-less host may fall back to host-key-only is now
  a v1 blocker**, because refusing wholesale removes Linux VPS support on the
  providers that do not expose one, which is most of them.
- Record the credential key policy in the backup manifest and refuse a
  cross-generation restore. Newer systemd releases pin credentials to the
  platform key material in a way that older releases cannot read, so a backup
  taken on one generation may be unreadable on another. The same release floor
  governs the hardening directives used by the service unit.
- Decrypt credentials only for the server process at runtime. If a secure store
  is unavailable, refuse to add or use the connected service rather than saving
  its token in plaintext. After restore, require the owner to reconnect services.
- Treat a service as in use while enabled for a bot, referenced by a routine,
  or held by an active operation. Show dependencies and require them to be
  removed and active work to finish or stop before disconnecting. Apply this
  rule to agents and all other connected resources too.

### Server packaging and managed runtimes

- Bundle the matching `robokura-server` executable with each desktop app
  installer. The app starts it as a separate process; it does not download the
  local server on first use. **Linux is the exception**: the app attaches to an
  installer-provided systemd service instead of starting its own child, because
  the delegated cgroup the containment guardian needs cannot be obtained that
  way.
- Use `cargo-packager` for desktop installers and include the matching server
  executable as a packaged resource. Maintain a single guided Linux installer
  serving both VPS and desktop hosts; it installs the standalone server as an
  unprivileged `systemd` service and prepares the sandbox host prerequisites.
- The server manages Node.js/npm and `uv`/Python in its own runtime directory,
  installing them on demand without requiring global host installations.
- Prefer an agent binary, then `npx`, then `uvx`. Pin the agent version recorded
  by the ACP Registry.

### Agent artifact integrity

- Pin the exact agent version recorded by the ACP Registry.
- Verify a binary archive against the registry SHA-256 when one is present. If
  it is absent, show the source and version and require the owner's explicit
  confirmation before installation.
- For `npx` and `uvx`, use the package manager's integrity checks. Do not bypass
  those checks.
- Stop and report an integrity failure. Do not silently switch distributions
  after a checksum or package-integrity failure. Use the next distribution only
  when the preferred distribution is unavailable for the server platform.

## Confirmed technology choices

| Area | Choice | Reason |
| --- | --- | --- |
| Agent protocol | Stable ACP v1 through the official `agent-client-protocol` Rust SDK | Keeps agent sessions server-side and protocol-specific code behind one adapter. |
| Desktop notifications | `notify-rust` behind an internal interface | Covers the desktop OS targets while allowing platform-specific behavior to remain isolated. |
| Credential store | `keyring` crate `v1` API with native OS stores | Shares device credentials securely between the app and notifier without custom encryption or plaintext fallback. |
| Desktop packaging | `cargo-packager`; one guided Linux installer covering both Linux hosts | Produces native desktop packages and allows bundling the matching server binary. The Linux installer is shared by VPS and desktop because both install the server as a systemd service; they differ only in credential store. |
| Sandbox engine | MXC through `robokura-sandbox`, pinned exactly | One policy shape across three host families with explicit refusals for non-expressible fields; pinned and wrapped because the SDK is new and unsupported. Three host *families*, not three hosts: Linux is a single family with a VPS and a desktop shape. |
| JSON canonicalization | `serde_json_canonicalizer` | The alternative is documented as diverging from RFC 8785. |
| OpenAPI generation | `utoipa` with `utoipa-axum` | Builds the router and the specification from one route list, so a handler cannot ship without its contract. |
| SQLite driver | `rusqlite` with `bundled` and `backup` features | `bundled` fixes the SQLite version across hosts and avoids a distribution shipping an old engine; `backup` backs the consistent-snapshot requirement. |
| Token verifiers | Plain SHA-256 | 256-bit CSPRNG tokens have nothing to guess; a slow KDF adds a per-request cost amplifier and a pepper is forgeable from the same data directory. |

## Confirmed workspace boundaries

```text
robokura-app        Desktop window and user interaction
robokura-notifier   Companion process and operating-system notifications
robokura-client     Server connections, HTTP requests, and event stream
robokura-api        Versioned transport types
robokura-server     Server executable and process composition
robokura-core       Product rules, records, and persistence
robokura-acp        ACP protocol adapter and agent sessions
robokura-sandbox    Sandbox policy, backend adapters, containment, probes
```

Keep `robokura-core` responsible for product rules, records, and SQLite
storage. Do not add a separate storage crate until a concrete need for that
boundary appears. Keep `robokura-notifier` separate because it must outlive the
app window. Keep `robokura-sandbox` separate because the sandbox engine must be
swappable and because the enforcement guarantees in the architecture are the
sandbox crate's to own, not the domain's to assume.

The intended dependency direction is:

```text
robokura-app ───────> robokura-client ───────> robokura-api
robokura-notifier ──> robokura-client ───────> robokura-api
robokura-server ────> robokura-api
robokura-server ────> robokura-core
robokura-server ────> robokura-acp
robokura-server ────> robokura-sandbox
```

The core does not depend on the window, transport, ACP protocol, or sandbox
engine types. The server connects core services to the API, ACP adapter, and
sandbox crate.

### Command identity and idempotency

- Hash a command's method, normalized path, and canonical JSON body to bind its
  ID to exactly one operation. Malformed and unauthorized requests create no
  receipt; valid deterministic domain outcomes do. External-work receipts mean
  accepted, while the durable operation and events report completion.
- **Validate before canonicalizing, and do not trust the canonicalizer for
  validation.** Two failure modes silently break the one-ID-one-operation
  property if left unhandled. Duplicate object keys are accepted with
  last-occurrence-wins by the JSON parser and preserved rather than rejected by
  SQLite's JSON validation, so a custom deserializer rejects a repeated key at
  any depth. And large integer literals are silently narrowed to doubles, so two
  genuinely different bodies can canonicalize to the same string; every number
  must be an integer literal within the safe integer range, and fractions,
  exponents, negative zero, and non-finite values are rejected before hashing.
  Nesting depth is bounded, which also removes a cheap denial-of-service vector.
- Use `serde_json_canonicalizer` for RFC 8785. Do not use `serde_jcs`; the
  ecosystem documents RFC divergences from it.
- Store device and pairing verifiers as **plain SHA-256**. The tokens are 256
  bits from a CSPRNG, so a slow key-derivation function buys nothing and adds a
  per-request cost amplifier on the authentication path. A server-held pepper in
  an HMAC is actively worse here, because the database and the configuration file
  holding the pepper live in the same data directory, so an attacker who steals
  one steals both and can forge verifiers for arbitrary tokens. Save slow
  derivation for a future human-chosen passphrase feature.
- Lookup of a presented token is a single indexed probe on the verifier. The
  compared value is a digest output rather than the secret, so index-probe timing
  leaks nothing exploitable. Perform a dummy hash on an unknown token so an
  unknown and a revoked credential cost the same.
- Use lowercase canonical UUIDv7 text for Robokura IDs and client command IDs.
  Keep ACP IDs opaque. Version 7 generation is documented as monotonic within a
  process, which is sufficient because the server mints ordered IDs and the app
  and notifier use command IDs only as deduplication keys, never as ordering
  keys. Never paginate or sort by UUIDv7 time; use the explicit sequence columns.
- Use RFC 8785 JCS over validated request JSON and SHA-256 over a
  domain-separated method/path/body string for receipt fingerprints.

## Decisions still open

- Answer the Windows exact-file grant question by spike against the native
  no-DACL-mutation tier, then set the first release platform scope.
- Confirm cgroup v2 **delegation** for the server's service unit on every Linux
  VPS image intended for release, and validate the containment guardian against
  a detached child on each host. Together these are what the revocation
  guarantee rests on; the second without the first is untestable in production.
- Validate the bootstrap channel's Windows handle-list inheritance, including
  that a notifier spawned in the same window cannot read the channel.
- Qualify egress per agent and version: which agents reach their provider
  through the constrained path, and which authentication flows are usable
  headless. Every agent starts `unqualified`, and the `unqualified` first-run
  rule decides whether any bot can start at all, so it is decided rather than
  defaulted.
- Validate desktop OS credential stores and Linux VPS credential storage and
  rotation, including which hosts without a virtual TPM must fall back to a
  host-key-only policy or be refused. On Linux **desktop**, confirm the secret
  service exists before the installer reports the host as ready.
- Validate notification behavior, credential-store backends, and packaging on
  each target OS during implementation planning.
- Add the paged owner notification list and the owner-set notification retention
  bound. The `robokura-notifier` crate that ships in the first release depends
  on both, and neither is specified yet.

## Protocol references

- [ACP v1 overview](https://agentclientprotocol.com/protocol/v1/overview),
  [terminals](https://agentclientprotocol.com/protocol/v1/terminals), and
  [ACP Registry authentication methods](https://github.com/agentclientprotocol/registry/blob/main/AUTHENTICATION.md)
- [RFC 9562 UUIDs](https://www.rfc-editor.org/rfc/rfc9562.html),
  [RFC 8785 JSON Canonicalization Scheme](https://www.rfc-editor.org/rfc/rfc8785.html),
  and [RFC 9530 HTTP Digest Fields](https://www.rfc-editor.org/rfc/rfc9530.html)
- Sandbox references: [MXC repository and backend matrix](https://github.com/microsoft/mxc), [MXC Windows OS-version support](https://github.com/microsoft/mxc/blob/main/docs/backends/process-container/os-version-support.md), [MXC filesystem policy and fallback controls](https://github.com/microsoft/mxc/blob/main/docs/schema.md), [MXC macOS Seatbelt backend](https://github.com/microsoft/mxc/blob/main/docs/backends/seatbelt/seatbelt-backend.md), [MXC Linux Bubblewrap backend](https://github.com/microsoft/mxc/blob/main/docs/backends/bwrap/bubblewrap-backend.md), [MXC ProcessContainer networking](https://github.com/microsoft/mxc/blob/main/docs/backends/process-container/networking.md), [MXC host preparation](https://github.com/microsoft/mxc/blob/main/docs/backends/process-container/host-prep.md), [MXC support policy](https://github.com/microsoft/mxc/blob/main/SUPPORT.md), and [MXC Windows Sandbox backend and limitations](https://github.com/microsoft/mxc/blob/main/docs/backends/windows-sandbox/windows-sandbox.md). Note that the sandbox documentation is published under a `backends` layout, so links using other path shapes do not resolve.
- Isolation and host prerequisites: [Linux Landlock manual](https://man7.org/linux/man-pages/man7/landlock.7.html), [Linux kernel Landlock documentation](https://docs.kernel.org/userspace-api/landlock.html), [Ubuntu restricted unprivileged user namespaces](https://ubuntu.com/blog/ubuntu-23-10-restricted-unprivileged-user-namespaces), and [bubblewrap manual](https://manpages.debian.org/testing/bubblewrap/bwrap.1.en.html).
- Agent credential storage: [Claude Code authentication](https://code.claude.com/docs/en/authentication), [Gemini CLI authentication](https://geminicli.com/docs/get-started/authentication/), [goose known issues](https://goose-docs.ai/docs/troubleshooting/known-issues), and [ACP registry format](https://github.com/agentclientprotocol/registry/blob/main/FORMAT.md). The registry schema carries no authentication field, so agent authentication is discoverable only from a live handshake.
- Host credentials: [systemd credentials](https://systemd.io/CREDENTIALS), [`systemd-creds`](https://man7.org/linux/man-pages/man1/systemd-creds.1.html), and [`systemd.exec`](https://man7.org/linux/man-pages/man5/systemd.exec.5.html).
- Storage and archives: [`VACUUM INTO`](https://www.sqlite.org/lang_vacuum.html), [SQLite foreign keys](https://www.sqlite.org/foreignkeys.html), and [SQLite JSON functions](https://www.sqlite.org/json1.html).
