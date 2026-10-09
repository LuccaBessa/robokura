# Robokura Product and Architecture Plan

## Purpose

Robokura is a client for persistent personal bots that run on a Robokura Server
the user controls. The server can run on the user's computer or on a VPS. The
app connects to either server through the same client/server interface.

The bot is the lasting product entity. A conversation is one way to work with
it. An installed ACP agent provides the runtime a bot uses.

## Product model

- **Server:** Owns bots, conversations, memory, routines, installed agents,
  configuration, and execution state.
- **App:** Connects to a server, presents its bots and activity, and lets the
  user configure and talk to them.
- **Installed agent:** An ACP implementation installed and authenticated on a
  server. It exposes its own runtime options through ACP. Each server has a
  default installed agent for new bots and fallback reassignment.
- **Bot:** A persistent identity and configuration that uses an installed
  agent. Multiple bots may use one installed agent while keeping separate
  instructions, memory, routines, and settings.
- **Conversation:** Each bot has one private conversation with its owner. Bots
  may also participate in bot-to-bot conversations and group chats.
- **Group chat:** A conversation owned by the user with multiple selected bots
  participating in the same shared history. The user can bring several bots
  together to discuss a subject in one place.
- **Server connection:** A saved address and authentication context in the app.
  A connection selects which server's bots and resources the app shows.

Each server has one owner in the first release. That owner may connect from
multiple app installations. Multi-user access and shared ownership are out of
scope until the single-owner model works well.

Bots can continue working when the app is closed. Bot identity and conversation
history survive a server restart. The server should restore active bot work
where the selected agent supports session loading or resuming. ACP agents expose
these as optional capabilities, so recovery behavior for agents without them
still needs a product decision.

## System shape

```text
Robokura App
  GPUI / GPUI Kit
       │
       │ HTTPS API and WebSocket events
       ▼
Robokura Server
  API, authentication, domain services, storage
       │
       ├── ACP agent sessions
       └── server-side bot files and resources
```

The app never speaks ACP directly. The server owns ACP sessions and translates
protocol updates into stable Robokura events for the app. The app can reconnect
and recover current state from the server after a network interruption.

Local and remote modes use the same interface:

```text
Local:  App → loopback connection → Server on the same computer
Remote: App → authenticated network connection → Server on a VPS
```

The local server should be a separately runnable program. The app may provide
controls to connect to and manage a local server, but the server must also work
without the app so background bot work can continue.

### Local server lifecycle

When the app opens in local mode, it checks the local server's health endpoint.
If the server is not running, the app starts it and connects after it becomes
ready. If it is already running, the app connects to it.

Closing the main window hides the app to the system tray. It does not stop the
local server or notification process. The tray explains that the server is
still running and offers an explicit action to shut it down. The user can also
reopen the app from the tray. The server saves held state during a graceful
shutdown.

Remote connections do not change the remote server's lifecycle. Closing the app
window hides the app to the tray, while the remote server continues on its own
machine. Quitting the app leaves the notification process running while the
remote server runs. The tray should not present remote shutdown as a local
server action.

### Local server detection decision

When the app finds a compatible Robokura Server already listening locally,
connect to it instead of starting a duplicate. Show whether the app started the
server or found it already running. Keep an independently started server alive
when the app exits; only stop it after the owner explicitly chooses the local
server shutdown action. That action ends notifications for the local server but
leaves the background process active for any other running selected server. If
the detected server is incompatible or cannot be authenticated, explain the
issue instead of starting a second server.

### Authentication decision

Keep the server vendor-neutral. It exposes a normal endpoint, and Robokura
provides the owner authentication for remote connections. Local mode connects
to a loopback-only server. The authentication flow should stay small and should
not require an external network service. Agent authentication with model
providers remains separate from app-to-server authentication.

For a remote server that is already reachable, use direct pairing: the owner
creates a one-time pairing code through the server CLI over SSH or its local
console, then enters it in the app with the server address. The code is
single-use, expires after ten minutes, and is accepted only over HTTPS with
normal certificate validation. Each paired app installation receives its own
revocable credential once; the server stores only a verifier. For the first
local pairing, the app receives a one-time bootstrap secret through a private
control channel from the server process it starts. Avoid account creation, a
central identity service, or a managed relay. The owner provides network
reachability and TLS for the server.

Support SSH tunneling as an optional remote connection path for a server
reachable over SSH, so its API port does not need to be exposed directly. Keep
direct HTTPS pairing as the primary path. Keep HTTPS and certificate
validation end-to-end inside the SSH tunnel. Neither connection path should
require a third-party network service. Start the tunnel when the owner connects
to that server, reconnect it if the connection drops, and show its connection
state in the app. Use the owner's existing SSH agent and host configuration
for SSH authentication. Robokura does not store SSH passwords or private keys.

The pairing screen, CLI command wording, and setup guidance remain to be
designed.

## Proposed Rust boundaries

```text
robokura-app       GPUI window and interaction
robokura-notifier  Companion process and OS notifications
robokura-client    HTTP, WebSocket, connection state, API client
robokura-server    Server binary, API, authentication, startup and shutdown
robokura-api       Shared versioned requests, responses, and events
robokura-core      Product records, domain rules, persistence-facing services
robokura-acp       ACP agent installation and session adapter
robokura-sandbox   Sandbox policy, per-host backend, containment, probes
```

Dependency direction:

```text
robokura-app → robokura-client
robokura-notifier → robokura-client
robokura-client → robokura-api
robokura-server → robokura-api
robokura-server → robokura-core
robokura-server → robokura-acp → ACP
robokura-server → robokura-sandbox
```

The domain should not depend on GPUI, network transport, ACP protocol types, or
the sandbox engine. The API crate carries transport-safe contracts, not domain
behavior. The server joins domain services to ACP, the sandbox crate, and network
transport. The app draws server state and sends user actions; it does not own
product records. The sandbox crate is separate so the enforcement engine stays
swappable and so the guarantees the product claims about isolation belong to
one place rather than being assumed by the domain.

## Core bot workflow release scope

The initial release delivers a safe, end-to-end workflow for one owner to use
individual bots on a local or remote Robokura Server:

1. Open the app and attach to a healthy local server or pair with a remote one.
2. Browse the ACP Registry through the server, install an agent, authenticate
   it, and configure the options it exposes.
3. Create a bot with a name, purpose, and instructions, and assign its agent.
4. Send messages in the bot's single private owner conversation and receive
   live responses and structured activity.
5. Keep bot work running when the app closes, recover unfinished work after a
   server restart when safe, and notify the owner when attention is needed.
6. Run bots inside the default sandbox. Allow explicit read/write access to
   owner-selected files or directories for the current task only; expire that
   grant on completion, cancellation, or failure. Require owner approval for
   high-impact actions.

### First-release server hosts

Four hosts are in the initial release:

| Host | Server start | Credential store |
| --- | --- | --- |
| macOS desktop | app-started | OS credential store |
| Windows 11 desktop | app-started, constrained rather than verified | OS credential store |
| Linux VPS | guided installer → unprivileged systemd system service | systemd encrypted credentials |
| Linux desktop | guided installer → unprivileged systemd system service | secret service (D-Bus) |

Linux VPS and Linux desktop are **same-operating-system, different-host** cases
and must not share a code path by assumption. They differ in how the server is
launched, which credential store is available, and which prerequisites are
default-on versus default-off, so each has its own gate row and its own tested
host list.

**Linux is a prepared-host platform.** A stock Linux machine fails the sandbox
gate, and this is expected rather than exceptional: Ubuntu 24.04 LTS restricts
unprivileged user namespaces through AppArmor by default, and `bubblewrap`,
`slirp4netns`, and the `iptables` front-end are not installed by default on
either desktop or VPS images. "We support Linux" therefore means "we support
documented, prepared hosts", and the owner-facing diagnose command and
capability report are the mechanism that tells an owner exactly which of the
prerequisites is missing rather than reporting an undifferentiated failure. The
alternative reading — Linux works out of the box — is not what this plan
promises and must not be implied in the app or in packaging copy.

Windows 11 desktop is listed but **constrained rather than verified**: the
native tier was measured once, on a host below the documented OS floor, and it
granted the containing directory rather than the requested file. That is a
known constraint to resolve while building `robokura-sandbox`, not a reason to
defer planning. See "Known Windows constraint" in `SYSTEM_ARCHITECTURE.md`.

The initial release excludes group chats and bot-to-bot conversations and
handoffs, which follow the core workflow. Routines, connected services, the
shared skill library, editable/automatic long-term memory, and browser control
also follow the core workflow. Keep these in the full product scope; their
behavior remains defined elsewhere in this plan. Backups and exports are also
later features.

### Release sequencing decision

Add user-created group chats and bot-to-bot conversations/handoffs after the
core bot workflow. Keep them in the full product scope, but exclude them from
the initial core release.

## Full product scope map

This map describes the full product. Items outside the core bot workflow
release scope are later features, not first-release commitments.

### Server and connections

- Create, start, stop, inspect, and update a local server.
- Connect the app to local or remote servers through one interface.
- Show server identity, health, version, and active work.
- Manage owner authentication and connected app installations.
- Keep one server owner while supporting the owner's multiple devices.
- **Deploy, configure, and update a Linux host or VPS.** First release: a guided
  installer that prepares the host and installs the server as an unprivileged
  systemd service. **Back up, restore, and diagnose a VPS server.** Later
  feature, as stated above; diagnostics still exist in the first release as the
  owner-visible capability report and diagnose command, which is what an
  unprepared Linux host needs most.

### ACP agent management

- Browse and search the ACP Registry for the selected server's platform.
- Inspect agent source, license, version, platform support, and distribution.
- Install, verify, authenticate, configure, update, and remove agents on the
  server.
- Show each installed agent's readiness, capabilities, and active bot use.
- Support agent-specific authentication and configuration without assuming
  every agent offers the same options.

### Bots

- Create a bot with an identity, purpose, and instructions.
- Choose an installed agent and configure the bot's runtime options.
- Edit, pause, archive, restore, or permanently delete a bot.
- Give bots their own memory, skills, routines, and computer access where
  those features are supported.
- Show whether a bot is idle, working, waiting for the owner, or unavailable.

### Conversations and collaboration

- Give each bot one private conversation with its owner.
- Let the owner create group chats with selected bots in one shared history.
- Let bots hold conversations with other bots.
- Preserve bot-to-bot conversations while at least one participant exists.
- Preserve group chats after bot removal and let the owner delete them.
- Coordinate multiple bot responses, mentions, turn order, and conversation
  visibility.
- Preserve messages and structured activity across app and server restarts.

### Background work and attention

- Continue bot work while the app window is closed.
- Resume bot state after a server restart, with clear handling for agents that
  cannot resume an active ACP session.
- Notify the owner when a bot finishes, fails, or needs a decision.
- Show queued work, active work, blocked work, and completion history.
- Let the owner pause, cancel, or redirect active work.

### Computers and capabilities

- Give each bot a persistent server-side computer or working environment.
- Provide filesystem, terminal, browser, network, and application capabilities
  according to server support.
- Set the resource and action policy for each bot.
- Present agent permission requests and other owner decisions in the app.
- Enforce access boundaries at the server or operating-system level where the
  product claims they are enforced.
- Show the server and computer where a bot performs work.

### Memory, skills, and routines

- Keep durable bot memory and let the owner inspect or edit it.
- Create skills manually or have a bot draft one from a completed task; review
  and edit drafts before saving them to the shared library.
- Add, import, remove, and share reusable skills.
- Schedule routines and define their triggers, inputs, and results.
- Review routine activity and pause or remove a routine.
- Decide which memory, skills, and routines can be shared across bots.

### Data and portability

- Back up and restore a server as a consistent unit.
- Export readable bot and conversation data separately from full backups.
- Delete bots, conversations, group chats, memory, and resources according to
  explicit ownership rules.
- Exclude agent-provider credentials from ordinary exports and define their
  treatment in backups.
- Start the server with an empty store. The schema starts at version one.

### App experience

- Keep the app connected to a selected server and make server context visible.
- Organize bots, direct conversations, group chats, and activity.
- Provide search, notifications, settings, and connection management.
- Close the window to the tray while keeping the local server running.
- Let the owner explicitly shut down the local server from the tray.

## Product and implementation questions

### Now decided

- **Which computer capabilities each host can enforce.** Support is expressed as
  a host profile, an enforcement backend, and runtime prerequisites, and the
  normative statement is that a host may execute bots only when its runtime probe
  passes. Distribution names appear only in a non-normative list of tested
  images, because the same image behaves differently depending on whether a
  namespace restriction is in force. Hosts that fail a required check manage and
  pair but do not run bots, and the app says so rather than running a bot
  without the boundary it claims.
- **Whether the loopback proxy can enforce provider destinations.** Resolved
  structurally: provider egress rests on a kernel-enforced deny with a single
  loopback exception, so an agent that ignores proxy settings fails to reach its
  provider rather than reaching the internet. What remains is per-agent
  qualification, recorded per agent, version, and flow. An agent that cannot be
  configured for the constrained path has that flow marked unavailable and its
  runs refused; the sandbox policy is never widened to accommodate it.
- **How browser automation and remote viewing work.** Browser control is out of
  the first release; only the capability key ships, so the policy can express and
  refuse a browser grant without ever silently widening it. There is deliberately
  **no** browser probe: `unavailable` in the capability report means a baseline
  check ran and failed, and a browser that was never built is not a check that
  failed. The key is absent from the baseline checks entirely and only reachable
  as a request-time capability refusal, which is what keeps `unknown` from ever
  being able to mean "available". The browser runs outside the per-run sandbox
  when it exists, because a persistent profile cannot coexist with a per-run
  immutable filesystem policy.
- **How bots start bot-to-bot conversations.** One stdio MCP server supplied to
  the agent at session creation, which is the only tool transport the protocol
  requires of every agent. Because it runs with a per-run token, the agent
  cannot name its own identity and so cannot impersonate another bot, and a
  handoff grants no capability and needs no approval.
- **Which diagnostics the owner can inspect.** A server-side diagnose command and
  an owner-visible capability report cover it. Support bundles include manifests,
  host state, probe transcripts, and coded failures, and exclude credentials,
  message text, workspace bytes, and host paths. Nothing is retained
  automatically; the bundle exists where the owner asks for it.

### Still open

- Which server deployment and update paths the product supports.
- Whether an agent's first run may proceed while `provider_path_verified` is
  `unqualified`, and what the app discloses when it does. Qualification is
  empirical and the first run is the experiment, so this must be decided before
  any bot can start at all.
- Whether the paged owner notification list and an owner-set notification
  retention bound ship in the first release. The notifier's missed-notification
  refresh depends on both, and `robokura-notifier` is a first-release crate.

## Computer model proposal

The server host is the physical computer. A bot's computer is its persistent
working environment on that host, not necessarily a separate virtual machine.
It includes a working directory, files, installed tools, and the capabilities
the server makes available to the agent.

The main design choice is whether bots use separate logical workspaces under one
server account or isolated operating-system environments. Group chats share
conversation history by default. They do not share bot workspaces or files
unless the owner grants selected bots access to a shared location.

ACP gives the server a working-directory setting and optional client-provided
terminal methods. These describe where a session works and how commands run;
they do not by themselves provide operating-system isolation.

### Sandbox and file access rules

Run each bot's agent and child processes inside an operating-system-enforced
boundary by default. The bot can work only inside its own persistent workspace.

If the owner selects a file or directory outside that workspace for a task,
grant that bot access to the selected path for that task only. Revoke the grant
with read and write access when the task completes, is canceled, or fails. The
owner can revoke the grant earlier at any time. Revocation blocks new access
immediately by stopping the run and its entire contained process tree. Do not
report revocation complete until the sandbox confirms the process tree has
exited and can no longer use the path. Changes already written are not rolled
back. A group-chat participant receives no file access just because it can
read the shared conversation; the owner must grant the path to the selected
bot or bots.

Define a work run as one bounded unit of execution, created by an owner message, a
routine trigger, or a bot handoff. A group-chat message creates a coordinating
run; each participating bot gets its own child run. A handoff also creates a
child run, and permissions do not transfer to the receiving bot. Runs move
through queued, running, waiting-for-owner, and stopping states, then end as
completed, canceled, or failed. A server restart continues the same run when
safe.

The coordinating run owns the turn budget, so the limit is global to the
conversation rather than per bot. Every bot turn and every handoff increments it
on the coordinating run, not on the child. At the limit the coordinating run
pauses for the owner to extend it or end it; reaching the limit never
auto-continues. The budget exists only where a coordinating run exists, so an
ordinary owner-and-bot conversation is never subject to one.

Treat ACP permission prompts as a separate interaction layer. They can explain
an agent's requested action, but the sandbox and server policy enforce what the
process can reach. Network access and host application control need separate
capabilities because a filesystem boundary does not control them.

Operating systems provide different isolation mechanisms and restrictions. The
server must refuse to start a bot when enforced isolation is unavailable. It
must never silently run without the boundary it claims to provide.

### What the isolation does and does not promise

Be precise with owners about what the boundary buys, because a guarantee that is
not enforced is worse than a smaller one that is.

Every host denies filesystem access by default, confines a bot to its own
workspace plus exactly the paths the owner selected, and confines network access
to a single server-managed proxy. Those are real, enforced properties.

Grant binding is not race-free. The host binds a selected path at launch rather
than to a filesystem object, so a narrow window exists between validating a
selection and starting the process against it. Robokura records the target's
identity, re-checks it from inside the sandbox immediately before the agent
starts, and refuses the launch if it changed. The worst case of losing that race
is access to exactly one unintended file, not general host access, because the
deny side is authoritative everywhere.

On macOS the boundary defends against a bot reaching further than it was given,
not against a determined attacker controlling the model and its tools. macOS
sandboxing is known to be escapable by a process inside the boundary, and it
allows reading metadata such as existence and size for arbitrary host paths.
The app therefore reports macOS execution as limited rather than full, and the
product does not describe it as a defence against a hostile agent. Its network
confinement, by contrast, is enforced by the kernel and is strong.

Windows local execution depends on one unanswered question: whether the
operating system can grant a process access to a single individual file, with
read and write, without modifying access control on the host. If it cannot,
Windows servers manage and pair but do not run bots, and the app says so plainly
rather than silently widening a file grant to its parent directory.

### Selecting paths on the connected server

The app's file picker browses the filesystem of the selected Robokura Server.
It never treats a path on the app computer as a path on a remote server. Show
the server identity and the server-side path throughout browsing and
confirmation. Start browsing at the server account's home directory; the owner
can add server-side browse roots by entering a path on that server. A browse
root only controls what the owner can select and does not grant a bot access.

Do not follow symbolic links or Windows reparse points while browsing. Show
them as unavailable selections. Before submission, confirm the exact resolved
server path, whether it is a file or directory, the selected bot, and the
read/write access being granted. A directory grant covers that directory's
tree, not its parent or siblings. The server resolves and records the target;
the client cannot grant access by submitting an arbitrary path string.

Create the message, work run, and confirmed path grants atomically before
starting the ACP process, so its sandbox policy includes those grants at
process launch. A grant cannot be added to an already-running process; the
owner can send a follow-up message that starts a new run with a new grant.
Revalidate the selected filesystem object at launch and fail closed if it has
been replaced or moved. The backend must bind enforcement to the validated
object without a path-check-to-launch race. If the host backend cannot enforce
the exact selected file or directory boundary, or cannot reliably stop every
process in the run, reject the grant or refuse to start the run rather than
widening access or claiming revocation succeeded.

### Computer capability layers

Keep capabilities separate so granting one does not silently grant the others:

- **Workspace:** The bot's persistent files and working directory inside its
  sandbox. A task-scoped file grant can add a selected outside path with
  temporary read and write access.
- **Terminal:** Commands and child processes run inside the bot's sandbox and
  use its workspace. A file grant exposes only the selected path; it does not
  grant access to the rest of the host.
- **Agent network:** The network access the ACP runtime needs to authenticate
  with and contact its model provider.
- **Web and external services:** Browser access, general network access, and
  connected services. Browser-based web access is enabled for bots by default.
  Connected services are available to bots by default once the owner connects
  them to the server. The owner must make each service connection, and can
  disable a service for a bot. Keep general outbound network access off; bots
  use the browser and connected services for external access. These
  capabilities remain separate from agent-provider connectivity. Keep service
  tokens server-side in a dedicated credential store and mediate service calls
  through the server so ACP agents never receive raw tokens. Exclude service
  credentials from backups and exports.

Treat a connected service as in use while it is enabled for any bot, referenced
by a routine, or held by an active service operation. Show those dependencies
and require the owner to disable the service for dependent bots/routines and
finish or stop active operations before disconnecting it. Apply the same
in-use protection to installed agents and all other connected resources.
- **Host applications:** Control of desktop applications or other host-level
  interfaces. This is a separate, platform-specific capability.

Host application control is a separate, platform-specific capability. Keep it
off by default and require the owner's explicit approval before a bot uses it.
The approval applies to one task and expires when the task completes, is
canceled, or fails. Its place in the first release remains open.

### Browser experience decision

Give each bot its own persistent browser profile. Keep its cookies, history, and
site sessions separate from the owner's personal browser and from other bots.
The owner can view the bot's browser, take control, pause it, sign out, or clear
its data. A task can also use a temporary clean profile when isolation from the
bot's saved sessions is useful. Group-chat membership does not share browser
profiles. For a remote server, the app provides the view and control path to the
browser running on that server.

Browser control is not part of the first release. The capability key exists in
the policy from the start so a browser grant can be expressed and refused, but no
browser is installed, launched, or exposed. Shipping a browser-managed runtime,
per-bot profile storage, and a remote-control transport alongside per-host
sandbox validation would put the first release at risk, and the plan already
places browser control after the core workflow.

Two constraints are recorded now because retrofitting them is expensive. The
browser runs **outside** the per-run sandbox, on its own boundary, because a
persistent profile cannot coexist with a per-run immutable filesystem policy and
because a browser's process tree and memory footprint make per-run containment
proof much weaker than for a single agent process. And the browser's profile
directory must already be excluded from backups, since site cookies and sessions
are secrets that are host-specific and must be re-established by signing in
again. Remote viewing relies on an experimental protocol surface while remote
input dispatch does not, so if browser control is brought forward, owner control
should arrive before owner viewing.

Browser access remains a separate capability from agent-provider network
access. This decision describes the intended experience; browser automation,
remote viewing, and host enforcement still need technical design.

## Memory, skills, and routines proposal

Keep these as three distinct things in the product:

- **Memory** is information a bot may carry between conversations and tasks.
  It belongs to one bot by default. The owner can inspect, edit, and delete it.
  A bot must not silently turn an entire conversation into permanent memory.
- **Skill** is a reusable set of instructions and supporting files that helps
  bots perform a kind of work. Keep skills in one server library so the owner
  can reuse them across bots. Skills do not grant capabilities or file access
  by themselves; a bot still needs the relevant access and connected services.
  Use a portable `SKILL.md` folder format with reference files only. Skills
  contain no executable scripts.
- **Routine** is an owner-defined trigger and task that starts bot work without
  a new message from the owner, such as a schedule or supported event. It can
  invoke a skill, belongs to a bot, can be paused or removed, and records each
  run and its outcome. Routines can use both scheduled and event-based triggers.
  They use the bot's existing capability policy and must not gain broader
  access just because they run unattended. Event triggers can come from files
  in the bot's workspace or an explicitly granted folder, and from connected
  services. The owner configures each trigger explicitly.

Keep memory bot-specific by default. A group chat shares its conversation only;
it does not create shared memory. Routine output should appear in the bot's
conversation or activity history so the owner can inspect what happened.

### Memory behavior decision

Bots may save useful information to their own memory automatically. Keep memory
separate from conversation history and show it in the bot's settings so the
owner can inspect, edit, or remove it. The owner can also tell a bot to remember
or forget something. Do not automatically turn the full transcript into
memory. Group-chat messages and routine output do not become long-term memory
by default.

### Memory, skills, and routines decisions still open

- Whether the first release includes editable memory, the shared skill library,
  or routines.

### Bot collaboration decision

Let a bot asynchronously hand work to another bot. The receiving bot wakes,
handles the request, and can reply later. Make the handoff visible to the owner
in the conversation where the work is coordinated. In a user-created group
chat, selected bots can respond and hand work to each other in the shared
conversation. A normal message lets the bots decide who should respond. The
owner can mention one bot to direct a request or mention several when each
should contribute. Keep group-chat creation and deletion under the owner's
control.

For an unmentioned message, invite all participating bots to decide whether
they have a useful response; a bot may abstain without posting. An explicit
mention invites only the named bot or bots.

The turn limit is a **group-chat control**, not a general run limit. It applies
when a coordinating run exists, starts at eight, and counts every bot turn
including handoffs. At the limit the coordinating run pauses for the owner to
extend it or end it; reaching the limit never auto-continues. The limit belongs to
the coordinating run, so it is shared across the whole conversation rather than
reset per bot.

A normal owner-and-bot conversation has **no** turn budget. There is nothing to
bound: one bot takes one turn at a time and the owner is present to stop it. The
budget exists because a group thread can keep itself going without the owner
saying anything, and unbounded autonomy is what the limit is there to prevent.
Confusing the two would put an arbitrary pause in front of every ordinary
conversation.

Abstaining needs a product answer because the protocol has none: an agent's stop
reason has no "nothing to add" value. Provide an explicit decline tool so a bot
can say nothing on purpose, record a reason, and still avoid posting. Fall back
to a client-metadata flag, which the protocol reserves for exactly this kind of
extension, and only then to inferring it from a turn that ended with no new
message. Abstention never appears in the transcript as a message.

Bot-to-bot conversations follow the same idea for identity: one conversation per
ordered pair of bots, created on the first handoff and reused for the life of the
pair, which keeps the retention rule simple.

### Routine approval decision

When an unattended routine reaches an action that needs the owner's approval,
pause that run and notify the owner. Wait for an explicit decision before the
action proceeds. The owner sets how long the run waits. If the wait expires,
end the run without performing the gated action. A timeout never counts as
approval.

### Skill format decision

Store each skill as a portable folder with a `SKILL.md` file and optional
reference files. Skills contain instructions and reference material only; they
do not contain executable scripts. The shared library can serve a skill to any
bot, while the server shows whether the bot's selected agent can use it.

When an agent cannot use a skill, show the skill as unavailable for that bot and
explain why. Do not silently omit a selected skill from the agent's context.

### Skill authoring decision

Let the owner create a skill manually or ask a bot to draft one from a completed
task. A generated skill stays a draft until the owner reviews and saves it to
the shared library. The owner can edit saved skills later. Let the owner import
existing skill folders into the library, and review imported skills before
making them available to bots. Let the owner export skills as portable folders.

### Notifications decision

Keep a Robokura background process running for notifications while the main
window is closed or the app is quit. It stays connected to the owner's selected
servers so it can deliver native notifications for approvals, work that needs
the owner's attention, and completed background tasks, including work on remote
servers. Do not notify for routine progress updates. When connectivity returns,
it refreshes pending requests and activity from the server. The notification
process stays alive while any selected server is running and stops when all
selected servers have stopped, not when the owner quits the app. Define the
process lifecycle and supported platform behavior during technical design. Let
the owner mute notifications per server or per bot and control approvals,
attention requests, and completion notifications as separate categories. After
a connection returns, notify about outstanding approvals and attention
requests, then summarize completed work rather than replaying every missed
notification. Keep an in-app notification history so the owner can review
activity after reopening the app.

## Data ownership

The server is the source of truth for bots, conversations, agent installations,
agent configuration, and execution state. Its database and bot data live on the
server host. A VPS connection therefore uses the VPS's installed agents and
files, not the app user's local files.

The app stores only connection profiles and client preferences. It should not
silently copy server-owned bot data into a second authoritative store.

## Data lifecycle

Keep durable Robokura data on the server. A server owns one database and a
server data directory for agent installations and bot resources. The app keeps
only its server connections and presentation preferences.

Persist bot identities, conversations, bot-to-bot conversations, messages,
structured activity, and unfinished turn state as work happens. A server
restart must not turn an unfinished turn into a completed one. Agent binaries
can be reinstalled from their recorded source and version. Agent-provider
credentials stay server-side and are excluded from ordinary exports.

The owner should be able to back up and restore one server as a consistent
unit. The backup must include its database and bot-owned resources. Exporting
readable bot and conversation data should be separate from making a full server
backup. Do not add automatic retention or cleanup until the owner can inspect
and control what it removes.

### Restart recovery decision

Keep an active turn unfinished across a server restart. If its ACP agent
supports loading or resuming the session, restore that session and continue the
turn. Otherwise, start a fresh ACP session with the original request, relevant
conversation history, and saved activity. Tell the agent that work was
interrupted and have it inspect the current state before repeating actions. If
it cannot safely continue, leave the turn paused and ask the owner what to do.

ACP session loading and resuming are optional agent capabilities, so Robokura
cannot promise every agent restores its exact in-memory state.

Removing an installed agent should not remove bots or conversation history.
The server should require bot reassignment or explicit confirmation before it
removes an agent that bots still use.

Deleting one bot preserves its bot-to-bot conversations while another
participating bot still exists. The retained conversation keeps the deleted
bot's identity as a historical participant, but that bot cannot receive new
messages. Delete a bot-to-bot conversation only after every participating bot
has been deleted.

Group chats belong to the user and remain after their bots are deleted. The
user can delete a group chat and its history separately. Archiving a bot hides
it while keeping its data available for restoration. Permanently deleting a bot
removes its bot-specific instructions, memory, settings, and private
owner-facing conversation after stopping its active work.

### Backup, export, and restore proposal

Keep a restorable server backup separate from a readable data export. A backup
should capture one consistent point in time and include the database, bot-owned
resources, and installed-agent manifests with pinned versions. Restore should
reinstall agents for the target server rather than assume the original host's
binaries can run there.

A readable export should let the owner take selected bot profiles and
conversation histories out of Robokura without copying agent executables or
provider credentials. Provide Markdown for reading transcripts and a versioned
machine-readable format for importing selected bot profiles and conversations
into another Robokura Server. Include files attached to or created in the
exported conversations so imported history retains its files. The shared skill
library is exported separately.

Exclude agent-provider credentials from backups and exports. After restoring a
server, the owner authenticates its agents again. Validate that the server can
separate credentials from other agent data so backups and exports can omit them.

Credential exclusion must be structural rather than procedural, because agents
cannot be relied on to keep credentials out of reach. Several published agents
read a dotenv file from the working directory and its parents, at least one
silently falls back from the operating-system credential store to a plaintext
file when that store is unavailable, and at least one stores a credential class
outside the directory its documented configuration override relocates. A bot
that writes a token into its own workspace has therefore placed it inside the
backup set, and no server-side secret handling prevents that.

So each work run gets a fresh ephemeral home and configuration directory that
disappear with the run; an agent that must persist a refreshable login is given
a per-agent credential location that lives outside the backup set; a declared
exclusion manifest names every excluded path with a reason; and before a backup
writes anything, a content scan checks candidate files for credential shapes. If
a credential-shaped file is found inside a bot workspace the **backup fails**,
naming the workspace-relative path and the shape that matched but never the
value. The owner then either tells the bot to move the file or adds an explicit
recorded exclusion. There is no force option: silently including leaks a
credential, and silently dropping leaves the backup's copy of the workspace
diverging from the live one. A backup that fails is recoverable; a leaked
credential is not.

### Backup and export scope decision

The full product includes both restorable server backups and readable exports.
They remain separate features. Their first-release timing is part of MVP scope
selection after the full product scope is mapped.

### Export format decision

Provide readable Markdown exports and a versioned machine-readable export for
importing selected bot profiles and conversations into another Robokura Server.
Exporting a bot includes every conversation it participated in: its owner
conversation, bot-to-bot conversations, and group chats. Export each conversation
once even when more than one selected bot participated in it.
Include the bot's memory and routine definitions with its profile. Agent-provider
credentials are excluded, so the owner authenticates agents again after import.
Imported routines start paused and require the owner to review and enable them.
Import bots as new records with fresh IDs. If a name already exists on the
destination server, adjust the imported bot's name instead of overwriting or
merging existing data.
Keep participants whose bot profiles were not selected as historical references
in exported conversations. Preserve their names and messages without importing
their bot profiles.
Before importing, show a preview of the bots and the conversations, files,
memories, and paused routines that will be added. Import only after the owner
confirms.

Use a versioned JSON bundle for machine-readable exports. Keep its format
versioned independently from the server database schema.

### Backup encryption decision

Keep portable full backups unencrypted. Include version and integrity metadata,
and validate the backup before restoring it. Agent-provider credentials remain
excluded. Encryption keys would have to be backed up alongside the data or
delivered out of band, which reintroduces the credential custody problem the
exclusion rules exist to remove; owners who need encryption should place the
backup on an encrypted volume.

### Backup and export bundle layout

A full backup is a single streamed archive with its manifest as the first entry,
a consistent snapshot of the database, per-bot workspaces, the skill library, and
immutable attachment bytes. Installed agents are recorded as manifests carrying a
pinned version and integrity hash rather than as binaries, because the plan
already requires restore to reinstall agents for the target server rather than
assume the original host's binaries run there. The manifest carries the bundle
format version, product and schema versions, server identity, creation time,
source platform, per-entry size and digest, the exclusion list with reasons, and
an integrity block.

Two exclusions go beyond the credentials already named. **Command receipts are
excluded and purged from the restored copy**, because otherwise a restored
server would answer a retried command with the *previous* server's recorded
outcome, which is a correctness bug rather than a privacy one. **Stored external
agent session identifiers are cleared on restore**, because such an identifier
only means something to an agent installation holding the matching session on
disk, and restoring it onto a reinstalled agent produces a confusing restore
failure rather than a working session. **`work_runs.external_provider_ref` is
cleared on restore for the same reason**: it is an idempotency key that only
means something to the provider that issued it, and a restored server holding a
stale key could retry an external action under a key that no longer applies.

Restore validates the whole bundle in a staging directory before touching the
target, then swaps by atomic rename of the data directory. That is what makes
"leave the target unchanged if preparation fails" true by construction rather
than by careful sequencing: every fallible step happens in staging, and a
pre-restore backup of the existing server is required first. Restoring onto a
different operating system works, with three adjustments: agents are reinstalled
at their pinned versions resolved for the destination platform, the owner is
told that workspace contents may include host-specific files, and site logins
must be redone because browser profiles were never in the bundle.

A capability verdict is a **host fact, not data**, and it must never survive a
restore. `server_metadata.capability_report_json`, `capability_revision`, and
`capabilities_probed_at` all live in the database, and the database is in the
bundle, so restoring a bundle onto a different host would otherwise carry over a
verdict computed on the machine the bundle came from. A Linux bundle restored
onto Linux with no delegated cgroup would claim local execution is available when
it is not — exactly the failure the capability report exists to prevent. Server
identity and every other `server_metadata` field are preserved as data; those
three are not.

Therefore, in the restored copy:

- `capability_report_json` is cleared to an empty report and `capabilities_probed_at`
  to null. `capability_revision` resets to `0`.
- `execution_status` is written as `checking` and no bot is launchable until a
  full capability re-probe has run on the destination host.
- The app shows every restored bot as unavailable with a reason naming an
  unprobed host, rather than as ready-to-run, so nothing looks executable that
  isn't.
- The probe runs before the first launch is accepted and completes before the
  app reports the server as ready for bot work. If the destination host fails a
  required check, bots stay unavailable with that check's `reason_code`; server
  management, pairing, and remote connection all keep working, because a
  capability gap is never taken to mean the server is down.

Restoring onto the same host with the same backend therefore completes quickly
and simply re-confirms the report. It is restoring across a platform boundary
where this matters, and that is precisely the case the plain reading of "the
database is in the bundle" gets wrong.

A readable export is a separate artifact from a full backup: a versioned
machine-readable document plus Markdown renderings, carrying selected bot
profiles, their conversations, files, memories, and routines. Each conversation
is exported once even when several selected bots participated. Participants whose
profiles were not selected are preserved as historical references with their
names and messages intact and no bot profile. Import mints fresh identifiers for
every record, adjusts colliding names deterministically rather than overwriting,
forces imported routines to paused, and shows a preview of everything that will
be added before the owner confirms.

### Restore behavior decision

Treat a full restore as replacing the target server's data. Prepare and validate
the backup before changing the target. If validation or preparation fails, leave
the target unchanged.

Before restoring over a server with existing data, create a pre-restore backup.
If that backup cannot be created, stop the restore and leave the server
unchanged.

### Data lifecycle decisions still open

- Validate on real hosts that agent credentials stay out of backups while agent
  configuration and bot data restore normally, including the case where an agent
  wrote a token into its own workspace and the pre-backup scan refuses the
  backup.

## Agent lifecycle design

Agent discovery and installation belong to the selected server. The app browses
the ACP Registry through that server's platform context. Installing an agent
adds an ACP runtime to the server; it does not create a bot. A bot separately
selects an installed agent.

An installed agent should move through explicit states: available, installing,
installed but unauthenticated, ready, updating, or failed. After installation,
the server performs an ACP handshake and records the advertised protocol
version, authentication methods, capabilities, and configuration options.
Configuration options must come from the agent rather than a Robokura-wide list
of model or reasoning fields.

The ACP Registry allows each agent to publish one or more distribution types:
standalone binaries, npm packages through `npx`, and Python packages through
`uvx`. Agents do not necessarily publish all three. Keep the selected agent
version recorded so upgrades remain deliberate and diagnosable.

### Registry distribution decision

Support all ACP Registry distribution types: standalone binaries, `npx`, and
`uvx`, subject to the selected agent having a distribution for the server's
platform. Prefer a matching binary, then `npx`, then `uvx`. The server manages
its own Node.js and npm runtime for `npx` and its Python and `uv` runtime for
`uvx`, rather than relying on host installations.

Authentication is a server-side agent concern, but its interaction may involve
the app. ACP describes agent-managed and terminal-based authentication. A remote
server may have no desktop browser or interactive display, so remote login must
either be relayed through the app or be clearly unsupported for that agent.
Authentication credentials remain with the agent on the server host.

### Remote agent authentication decision

Keep the server as the ACP client and perform agent authentication on the
server host. For an ACP terminal authentication method, relay an interactive
terminal between the server and app so the owner can complete the agent's login
without moving the agent or its credentials to the app. For agent-managed
authentication, support the flow when the agent can complete it without a
server-side desktop, or when the agent exposes a remote-compatible flow. Show
an agent as unsupported for remote authentication when it requires direct
interaction with a server-side desktop and offers no compatible path.

ACP's terminal method asks the client to run the configured agent interactively,
while the protocol-driven method only specifies an authenticate request;
agents may have different login UX.

Rank the flows rather than treating authentication as one thing. **Browser-based
sign-in relayed to the owner's own machine is the preferred headless path**: ACP
lets an agent hand the client an out-of-band URL that the client opens, and
requires that a credential obtained this way never travel back over the protocol
or enter model context. That combination is exactly what a headless server
needs, because the browser lives on the owner's computer while the credential
stays on the server. Below it, **device-code and paste-the-code flows** are
supported wherever the agent offers them. **Terminal authentication** is
supported through the relayed terminal and is the fallback for interactive
command-line logins. **Provider environment credentials** are preferred where
documented, because they need no agent-managed state and work with an ephemeral
home directory. **A flow that needs a browser on the server host** is
unsupported: there is no way to proxy it without granting a browser capability.

One consequence deserves stating: a base-URL override is a policy hole, not a
convenience, because it changes where traffic goes. It is supported only when
Robokura resolves the address itself as a checked allowlist entry, never when it
comes from agent- or user-supplied text inside a run. And because several
agents require authentication before they will accept a new session at all, a
remote server cannot validate an agent's session-restore path until the owner
has authenticated once; until then the agent's restore capability is recorded as
unverified rather than supported.

Treat installation, authentication, configuration, bot assignment, update, and
removal as separate actions. An update should not silently interrupt active
bot work. Removing an agent that bots still use needs an explicit reassignment
or removal path.

### Agent update decision

Show available agent updates and require the owner to start them. Wait for
active sessions to finish before applying an update, or ask the owner to stop
them. Do not interrupt active bot work silently.

### Agent configuration decision

Store agent configuration defaults on the server's installation. Let each bot
override options for its own ACP sessions, and resolve the effective settings
when that bot starts or resumes a conversation. Do not add conversation-specific
configuration unless an agent option requires it.

When a bot is reassigned to another agent, keep settings the new agent supports
and show unsupported settings for owner review.

### Agent removal decision

When the owner removes an agent that bots still use, show the affected bots and
active sessions. Require active sessions to finish or be stopped, then reassign
those bots to the server's default agent. Keep the bots and their conversation
history, and remove the deleted agent's installation and server-side
authentication data. Before removing the default agent, require the owner to
choose a replacement. Do not delete bots as a side effect.

### ACP session scope decision

Keep one logical Robokura conversation per bot and participant thread. Run each
work run in a fresh sandboxed ACP process. When the agent advertises and
supports session loading, load the conversation's ACP session in that process.
Otherwise create a fresh ACP session and supply relevant context from the
durable conversation history and activity. The transcript remains the source
of truth; ACP restoration replay must not duplicate transcript entries.

Detect session capabilities from the installed agent's ACP handshake; do not
assume support based on its registry listing or name. Record compatibility for
the installed version and host. If session restoration is missing or fails,
continue with reconstructed context and make clear that exact ACP session
continuity is unavailable. Revalidate compatibility after an agent update.

## Security and operation

Remote access requires authentication and encrypted transport. The server
should bind to loopback by default for local use. Public exposure requires an
explicit server configuration and documented deployment guidance.

ACP agents execute on the server host and may have access to that host's
resources. Before implementation, define the server's filesystem and terminal
capability policy, approval behavior, and isolation boundary. The UI must show
which server a bot runs on so local and VPS execution are not confused.

Treat these as separate controls:

- **Robokura policy:** Which resources a bot is configured to use, and which
  requests require the owner's decision.
- **ACP permission requests:** Agent-originated requests that Robokura can
  present to the owner. ACP agents may request permission before a tool call,
  so this flow cannot be the only access boundary.
- **Host enforcement:** Operating-system or sandbox restrictions that limit
  what an agent process can access on the server.

The first release needs an explicit decision about which controls it provides
and what claims it makes about them. Do not describe a request prompt as a
filesystem or process sandbox.

### Approval policy decision

Let bots read and make ordinary changes inside their sandbox and any
task-scoped paths the owner explicitly grants. Require owner approval before
external or hard-to-reverse actions, such as sending or publishing content,
making a purchase, permanently deleting data, or changing a production system.
Apply the same approval boundary to routines. Present the exact action and its
target for review.

Server-side work must continue when a client disconnects. On reconnect, the
client first fetches the current bot and conversation state, then resumes the
live event stream. Persist enough structured agent activity to rebuild the
conversation view without flattening tool events into prose.

## Implementation approach

Build from zero against the decisions in this plan. The domain model, the
persistence model, the API surface, and the user interface are all designed
here, in the order given by the work sequence below. Nothing in this repository
implements Robokura yet, so these documents are the only specification: where an
implementation detail is undecided, resolve it by following the decisions and
boundaries recorded in them rather than by seeking an existing implementation to
copy.

The server schema begins at version one, migrations are numbered and
forward-only from `0001`, and no compatibility surface is carried. This is why
the architecture describes a migration runner with checksum enforcement rather
than an upgrade path, and why several sections state a single correct design
instead of listing alternatives to be chosen between later.

## Work sequence

1. Confirm product vocabulary, ownership boundaries, and the initial slice.
2. Decide authentication and resource policy for local and remote servers.
3. Specify API operations, event shapes, reconnect behavior, and versioning.
4. Define persistence records and migrations for servers, agents, bots,
   conversations, turns, and structured activity.
5. Build and validate the server and client foundations independently.
6. Connect the GPUI app to the local server and complete the initial slice.
7. Build the guided Linux installer for both VPS and desktop hosts, including
   host preparation and the delegated-cgroup service setup, and add remote
   connection guidance. This is a first-release deliverable rather than
   follow-on documentation, because Linux is a first-release host.

Three questions are recorded as **open, and they are resolved while building
rather than before it**. They change what ships rather than whether to start, so
the plan does not wait on them; each is written down next to the code that
answers it:

1. **Windows single-file grants.** Whether the Windows host can grant one
   individual file, read and write, without modifying host access control. No
   alternative avoids DACL mutation, so a negative answer removes Windows local
   execution from the first release. This is answered by the first working
   `robokura-sandbox` backend rather than by a design decision, and one
   measurement already points at the negative. See "Known Windows constraint"
   in `SYSTEM_ARCHITECTURE.md`.
2. **The containment guardian.** Whether it can reap a child process that has
   started its own session, on every supported host. This is what the
   revocation guarantee rests on, and on Linux it is also a deployment question,
   not only a code question: the cgroup v2 subtree the guardian drains must be
   **delegated and writable** by the server's service unit. A stock unit is not
   delegated, the drain primitive is unavailable, and there is no fallback that
   catches a `setsid`-escaping descendant - so a host without delegation would
   have to run without local execution at all. This is the one open question
   with a hard dependency on the Linux installer, which is why the installer is
   step 7 rather than an afterthought.
3. **The bootstrap channel on Windows.** Whether handle-list inheritance behaves
   as designed, and specifically that a notifier spawned in the same window
   cannot read the channel. The descriptor-3 convention is a hand-off across an
   API that does not guarantee it, so it is confirmed by an integration test
   rather than assumed.

None of these is a reason to delay the server, core domain, or app. The sandbox
crate is the only component whose design they change, and the server can be
built and tested with a sandbox that refuses to launch, which is its own
documented fail-closed behaviour.

## Decisions still open

None of these blocks drafting an implementation plan. They are listed because
each one is a decision or a measurement that the implementation has to make,
and writing them down here keeps them out of the code as undocumented
assumptions.

**Resolved while building, and worth knowing now:**

- **Windows exact-file grants.** Whether the Windows host can grant one
  individual file, read and write, without modifying host access control. One
  measurement says no - see "Known Windows constraint" in
  `SYSTEM_ARCHITECTURE.md`. A negative removes Windows local execution; there
  is no alternative that avoids DACL mutation.
- **Containment guardian against a detached child.** Whether it reaps a process
  that has started its own session, on every host. On Linux this depends on a
  **delegated and writable** cgroup v2 subtree, which is why the Linux server
  must be service-installed rather than app-started.
- **Which Linux images reach the systemd 250 floor**, and whether a vTPM-less
  host may use a host-key-only credential policy or must be refused. The floor
  is a hard prerequisite for credential storage, and refusing wholesale removes
  Linux VPS support on the providers that do not expose one, which is most.
- **Update mechanism.** The Linux server runs as a system service holding the
  delegated cgroup and the server identity, so an update has a defined
  relationship to that service. Manual re-install and an in-app trigger are both
  unstated.
- **Bootstrap channel on Windows.** Whether handle-list inheritance behaves as
  designed, including that a notifier spawned in the same window cannot read
  the channel.

**Resolved per agent and version, during implementation:**

- **Provider egress and auth flows.** Which agents reach their provider through
  the constrained path, and which authentication flows are usable on a headless
  server. Every agent starts `unqualified`, which is what lets the first run be
  the experiment that answers this.

**Still to be built:**

- The paged owner notification list and an owner-set notification retention
  bound. Both are required by the notifier that ships in the first release, and
  neither is specified yet.
- The verified capability matrix and the error-code catalogue, published as
  reviewable artifacts rather than prose.
- The later-feature contracts the current schema must anticipate:
  connected-service broker, routine missed-run and time-zone behavior, memory
  review, skill validation, and group-chat coordination.
