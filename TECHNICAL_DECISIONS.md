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
- Microsoft Execution Container (MXC) is the leading implementation candidate,
  not yet a selected dependency. Validate its Rust SDK and per-platform
  backends against per-run ACP process trees, persistent workspaces,
  task-scoped path grants and revocation, network controls, and Linux VPS
  prerequisites before selecting it.
- Current MXC docs give different host requirements: Windows ProcessContainer
  targets Windows 11 24H2+, macOS Seatbelt requires macOS 15+, and Linux's
  Bubblewrap backend requires host user namespaces and an installed `bwrap`.
  These are candidate floors, not Robokura support commitments; validate the
  documented host set and package the capability checks.
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
  directory-tree scope, race-resistant target binding, and complete process
  tree termination. The Windows candidate's documented OS floor and fallback
  behavior, macOS Seatbelt's path rules, and Linux Bubblewrap's mount policy
  are separate validation targets; they do not establish equivalent guarantees
  by themselves.
- Recommended support gate: Linux VPS uses Bubblewrap only when documented
  host prerequisites and all sandbox checks pass. macOS uses Seatbelt with
  direct egress denied and a server-managed loopback proxy for provider access;
  qualify every supported agent/auth flow to verify destination enforcement and
  that direct sockets remain blocked. Windows uses ProcessContainer only when runtime probing confirms
  native BaseContainer/PSEC enforcement for the complete requested policy;
  otherwise keep the server available for management and remote connections,
  but report local bot execution as unavailable. Do not use host-DACL mutation
  or Windows Sandbox as a silent fallback. Windows Sandbox maps directories,
  cannot isolate an individual selected file through its mapped-folder
  interface, permits one active VM per logon session, and documents best-effort
  teardown.
- ACP session restoration is capability-negotiated. Check `loadSession` before
  `session/load`; load replays conversation history, while `session/resume` is
  separate and also optional. Without an advertised usable restore capability,
  create a new session and provide bounded context from Robokura's durable
  history. Do not duplicate replayed content in the Robokura transcript or
  claim exact continuity for a reconstructed session. Probe and record
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
  exchanges the 10-minute code only in an HTTPS request body. For an app-started
  local server, deliver the bootstrap code over a private inherited control
  pipe. Keep pairing creation off unauthenticated HTTP routes.
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

### Connected services

- Store connected-service credentials server-side in a dedicated credential
  store, separate from ordinary product records. Route agent service calls
  through a server-side broker; never expose raw service tokens to ACP agents.
- Exclude connected-service credentials from backups and exports.
- Encrypt credentials at rest using platform-managed keys, without requiring an
  owner-entered master password for each unattended server restart. Use the OS
  credential store on desktop hosts. For a Linux VPS, evaluate systemd encrypted
  service credentials with a host key and TPM protection where available.
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
  local server on first use.
- Use `cargo-packager` for desktop installers and include the matching server
  executable as a packaged resource. Maintain a dedicated Linux VPS installer
  that installs the standalone server as an unprivileged `systemd` service.
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
| Desktop packaging | `cargo-packager`; dedicated Linux VPS installer | Produces native desktop packages and allows bundling the matching server binary; VPS service installation has its own guided flow. |

## Confirmed workspace boundaries

```text
robokura-app        Desktop window and user interaction
robokura-notifier   Companion process and operating-system notifications
robokura-client     Server connections, HTTP requests, and event stream
robokura-api        Versioned transport types
robokura-server     Server executable and process composition
robokura-core       Product rules, records, and persistence
robokura-acp        ACP protocol adapter and agent sessions
```

Keep `robokura-core` responsible for product rules, records, and SQLite
storage. Do not add a separate storage crate until a concrete need for that
boundary appears. Keep `robokura-notifier` separate because it must outlive the
app window.

The intended dependency direction is:

```text
robokura-app ───────> robokura-client ───────> robokura-api
robokura-notifier ──> robokura-client ───────> robokura-api
robokura-server ────> robokura-api
robokura-server ────> robokura-core
robokura-server ────> robokura-acp
```

The core does not depend on the window, transport, or ACP protocol types. The
server connects core services to the API and ACP adapter.

## Decisions still open

- Validate MXC and alternatives against the approved per-work-run ACP process
  boundary, host floors, child-process cleanup, revocation, and network
  behavior before selecting an implementation.
- Validate desktop OS credential stores and Linux VPS systemd credentials for
  connected-service secret storage and rotation.
- Validate the cross-platform private app/server bootstrap channel and produce
  exact API schemas and DDL from the contracts in `SYSTEM_ARCHITECTURE.md`.
- Validate notification behavior, credential-store backends, and packaging on
  each target OS during implementation planning.

## Protocol references

- [ACP v1 overview](https://agentclientprotocol.com/protocol/v1/overview),
  [terminals](https://agentclientprotocol.com/protocol/v1/terminals), and
  [ACP Registry authentication methods](https://github.com/agentclientprotocol/registry/blob/main/AUTHENTICATION.md)
- [RFC 9562 UUIDs](https://www.rfc-editor.org/rfc/rfc9562.html),
  [RFC 8785 JSON Canonicalization Scheme](https://www.rfc-editor.org/rfc/rfc8785.html),
  and [RFC 9530 HTTP Digest Fields](https://www.rfc-editor.org/rfc/rfc9530.html)
- Sandbox candidate references: [MXC Windows OS-version support](https://github.com/microsoft/mxc/blob/main/docs/process-container/os-version-support.md), [MXC filesystem policy and fallback controls](https://github.com/microsoft/mxc/blob/main/docs/schema.md), [MXC macOS Seatbelt backend](https://github.com/microsoft/mxc/blob/main/docs/macos-support/seatbelt-backend.md), and [MXC Linux Bubblewrap backend](https://github.com/microsoft/mxc/blob/main/docs/bwrap-support/bubblewrap-backend.md).
- [MXC Windows Sandbox backend and limitations](https://github.com/microsoft/mxc/blob/main/docs/windows-sandbox/windows-sandbox.md).
