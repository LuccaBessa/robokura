What the product is, and how a coding agent works in it.

## Priorities

1. Keep code simple, explicit, and maintainable.
2. Fix root causes, avoid temporary band-aids.
3. Preserve user changes, never revert unrelated edits.
4. Keep comments to a minimum.

## Words

- **Assistant**: what a person makes. A name, a title, a description, and the
  agent it runs on. A record this product owns.
- **Agent**: the program an assistant runs on, spoken to over the Agent Client
  Protocol. It brings its own sign-in and its own files. This product owns none
  of that, and an agent is never run as a command of this product's own.
- **Thread**: the conversation one assistant holds with the person. Exactly one,
  which the database enforces. No thread without an assistant, no second thread
  under one.
- **Message**: one thing in a thread. `Text` is something said. `Note` is a line
  of progress, such as a tool call reporting what it is doing.
- **Run**: the live connection to one assistant's agent, started when that
  assistant is first spoken to. It holds the reply while the turn goes on.
- **Turn**: one question and the reply it produces. A stopped turn keeps its row
  unfinished, so it reads as cut off rather than complete.
- **Probe**: a run belonging to no assistant, used once to ask an agent what it
  can be set to. It has no thread, and it is let go after twenty seconds.

## Crates

`robokura` -> `robokura-core` -> `robokura-acp`, and no arrow points back.

**`robokura-acp`** speaks the Agent Client Protocol and owns the only Tokio
runtime. It knows agents, sessions, and commands. It knows nothing about
assistants or threads, and it never reaches an agent by running a command.

**`robokura-core`** is everything that is not a window: the records, where they
are kept, what an assistant is sent, and the agents those assistants run on. It
compiles without GPUI Kit, and every check in it runs without a window. When it
needs something the interface holds, that is a finding to report rather than a
component to import.

**`robokura`** is the window, the panes, and `main`. It draws and keeps nothing
of its own.

Every source file opens with a `//!` header saying what it holds. Read that
rather than a list of file names, which is wrong the day the next file is added.

## What is kept

One SQLite file, found by `paths.rs` in the platform's own application data
directory. Write-ahead logging on and foreign keys on, so deleting an assistant
takes its thread with it. `assistants/<id>/` under the same root is also where
that agent is started, so one assistant's files cannot be found by another's.

The shape lives in migrations rather than in the file. A step is only ever added
at the end of the migration list, each in its own transaction, and `user_version`
is the position in that list. A message's place comes from `MAX(seq) + 1` and
not from a clock, so a reply still arriving holds its place.

## What a press does

1. The person's message is stored first, so it is on record even if nothing
   answers.
2. On the first turn only, the agent is started, and the purpose plus the history
   so far go with it. A later message is the message alone, because the running
   agent holds the rest.
3. An empty reply row is stored before the agent begins, so a turn cut off leaves
   a record of how far it got.
4. A message arriving during a turn waits for it rather than racing it.
5. One loop reads every running agent, fifty milliseconds while a reply is
   arriving and a quarter of a second otherwise, and writes back on a two hundred
   millisecond interval.
6. A reply's row is closed only when the agent finished it. A stopped turn or an
   agent that went away keeps its row unfinished. Anything still held is written
   back on close.

Nothing in that path decides anything while drawing.

## Drawing

Every control, row, field, card, header, bar, badge, separator, and menu is a
component from GPUI Kit. A hand-rolled `div` with a click handler on it is not a
component. `div` and `h_flex` and `v_flex` are for arranging kit components and
for the surfaces between them. Where the kit and the design disagree on a number,
change the number on the kit component.

Look it up at `https://gpui-kit.com/component/` before writing one. Each page
carries the import, the API, and worked examples. Reimplementing a component
inherits its bugs and loses its fixes: a hand-written title bar did not get the
kit's caption buttons, its platform handling, or its drag behaviour, and each
had to be rediscovered.

Where the kit genuinely cannot do something, that is a finding to raise, and the
finding says which component is missing and what it would have to do. The
assistant's own pane is already one: the sidebar takes only menu items and the
dock is a full panel system, so the pane is a column of kit components.

## Threading

- The app, the window, and the entity context are the UI thread unless work was
  moved off it. Blocking work goes to `cx.background_spawn` or
  `cx.background_executor`, and a synchronous blocking call is moved into a
  background task rather than awaited on the foreground one.
- `App::spawn`, `Context::spawn`, and `AsyncWindowContext::spawn` are polled on
  the main thread. Blocking or CPU-heavy work inside one is a dropped frame.
- Render is a pure state read. No filesystem, no store query, no process, no
  expensive recomputation.
- Handlers stay thin. `on_action`, subscriptions, listeners, key handlers, and
  the polling loop start background work and return.
- Cache what is expensive to load or compute and draw from the copy. The records
  are the exception: they are read on every redraw so the screen and the store
  cannot drift apart.
- A background task that ends an agent waits for the agent to have gone before
  anything touches what it was using. `Session::stop` is that path and it blocks,
  so it belongs on an action a person asked for rather than on a timer. Dropping
  a session is only the net under it.
- Never introduce Tokio outside `robokura-acp`.

Two questions, every time: **could this block?** and **could this run during
render?** If either is yes, it moves off the thread.

## Commands

- `cargo fmt --all`, checked with `-- --check`
- `cargo clippy --workspace --all-targets`
- `cargo test --workspace`, or `cargo test -p robokura-core` for a change in core
  or the protocol client, which skips the GPUI tree and the slow part of a build
- `cargo run -p robokura` for what a check cannot see
- `cargo test -p robokura-core --test live -- --ignored --nocapture` for the
  checks that need a real agent installed and signed in

Diagnostics are off unless asked for, and go to stderr. `ROBOKURA_LOG` takes a
filter.

## Rust

- No `unwrap()` or `expect()` in code that runs while a person is looking.
  `main.rs`, where the window could not be opened, is the exception, because
  there is no window to say it in. Checks use them freely.
- Errors are `robokura_core::Result` and `Error`. Use `?` and the `From` impls
  rather than `map_err`. `Display` says what happened in words somebody can read
  and keeps the underlying error for `source()` and the log.
- Do not swallow a failure with `unwrap_or_default()` where the failure means the
  work did not happen. Say it with a typed error and let the window show it.
- Ids are `String`. Wrapping an assistant, thread, or message id in a new type is
  the user's decision, because the schema, the prompt, and the protocol all
  carry them as text.
- **Never shell out.** An agent is spoken to over the protocol, not run as a
  command. There is no `Command::new` in this product.
- `pub(crate)` for anything shared inside a crate, and `lib.rs` re-exports what
  the outside of a crate needs. Keep modules focused and source files under
  roughly 500 lines. A type or a function exists in exactly one place, and
  splitting a file keeps every field and method that was reachable before.

A file is named for what it holds, and inside the window one is grouped into a
folder named for the region it sits in, with that folder's `mod.rs` saying only
what is in it. The file a component lives in is the only place that component is
written.

## Comments

- A comment earns its place by saying something the code does not. If deleting it
  leaves the reader with the same understanding, delete it.
- An inline comment explains why the line is there, or why it is not the obvious
  thing. Delete any that narrates.
- A doc comment on a public item says what the name does not.
- The load-bearing ones stay, and they are the ones a reader would otherwise have
  to rediscover. The retry in `remove_folder` and the shutdown in `Session::stop`
  are the two that get tidied away by mistake.
- Plain, active, and about this product rather than about Rust. No metaphor, no
  second person. An assertion message in a check says why the claim matters, in
  one line.

## Checks

Checks live in `crates/<crate>/tests/<subject>.rs`. There is no `#[cfg(test)]
mod` in `src/`, and no `#[test]` outside a `tests/` file. The file is named for
the subject, not for the function, so the name survives a rename.

Anything drawn belongs in `robokura/tests/window.rs`, unless the subject is a row
(`sidebar_item.rs`), a colour (`theme.rs`), the settings page, the transcript, or
how a time is said. Anything kept belongs in `robokura-core/tests/records.rs`,
what the agent is sent in `prompts.rs`, and finding or starting an agent in
`robokura-acp/tests/protocol.rs`. A new subject is a new file named for it.

A check lives in the crate that owns the subject, and may only reach what that
crate makes public. A check that needs a field the crate does not expose is a
finding to report, not a reason to widen the crate to `pub`. The checks that start
a real agent are in `robokura-core/tests/live.rs` and are `#[ignore]`d, so
`cargo test` never starts one.

Anything that draws gets a check that goes through the same path a person does,
written as something a person can do and see. `tests/window.rs` opens a real
window on a headless context over a store of its own, so a control, the bar's
toggle, and a question asked are covered without a person present.

Three things a check holds in place.

- **No colour at a call site.** `themes/robokura.json` is the only place a hex
  belongs. `tests/theme.rs` walks `src/` and fails on any other, and also on a
  theme role that is read but not named or named but not read.
- **No number in two files.** Every measurement two files must agree on is in
  `ui/layout.rs`.
- **No id twice, and no control without one.** Checks press by name, so a new
  control carries an `.id`. Two elements on one id make a lookup ambiguous, which
  is why the bar wraps its controls in a `drag_guard` with an id of its own. A row
  names its parts `assistant-{id}-name`, `-when`, `-last`, `-initials`, and a
  transcript row is `line-{index}`, because a row's own drawn state hangs off
  that id.

Run `cargo run -p robokura` for what a check cannot see: focus, the keyboard,
drag, the platform's own window furniture, and how a pane sits against the bar. A
check that finds an element is not the same as the layout being right.

## Features and dependencies

Every dependency version lives in the root `Cargo.toml` under
`[workspace.dependencies]`. Subcrates use `{ workspace = true }`.

The only feature in use is `gpui-kit`'s `test-support`, and it is a
dev-dependency of `robokura`, so the headless window is not compiled into what a
person runs. New features use `dep:crate_name` and `#[cfg(feature = "...")]` on
the module, the `use`, and every item that names its types.

**A feature must never switch off one of the parts that are not negotiable.** No
build of this product may be a build where the credential boundary is optional.
That boundary is in force now: the agent keeps its own sign-in, nothing here
holds a credential, and nothing advertises a client filesystem or terminal, so an
agent has no route through this application to the person's files. Isolation, and
a person's approval of an irreversible action, are deliberately absent from this
version. A feature may not become the thing that switches either of them off when
they arrive.

`[profile.dev]` strips debug info from dependencies because the GPUI tree is
large. Do not undo that to make one build faster.

## Common mistakes

- **Putting a window in core or the protocol client.** A store change must stay
  checkable without one.
- **Walking a thread to find its newest message.** `Store::previews` answers for
  all threads in one read and returns a cut of the message.
- **Truncating twice.** A preview is already cut by `PREVIEW_CHARS`, and the row
  ellipsises what it is given.
- **Caching what the store should answer.** Cache what the store does not already
  answer cheaply. Not the records.
- **Replacing the wording for a folder that will not go.**
  `Error::AgentStillRunning` says an agent is still holding its own folder,
  because somebody removing an assistant has no file in mind. The retry behind it
  waits for an agent just asked to stop, and shortening that wait turns a stated
  reason into the operating system's complaint.
- **A check that starts a real agent without `#[ignore]`.**
- **Naming a check after the function it calls.**
- **Writing and publishing in one path.** They are separate steps and the second
  is asked for.

Saying what went wrong has four paths, and each belongs to one place. The store
would not open: `Middle::Complaint` in the middle of the window, with the list
told too. Making an assistant failed: an `Alert` in the list. The agent would not
start, or a turn failed: `Core::problem`, drawn as a `Line::Trouble`. Anything the
interface can carry on with: `tracing::warn!`, and nothing on screen.

## Conventions

Prose follows the same rules: active voice, short sentences, no em-dashes, no
metaphors, no second person. The top-level `README.md` is customer-facing and
may address the reader directly. The product is described on its own terms, with
no other product named and nothing said about market, pricing, money, or
competition. A run is described by what it did and how long it took. No due dates
anywhere; progress is measured by working steps.

No changelog file is tracked and none is generated. Release notes are written when
a release is cut, and they say what changed for a person and what still cannot be
done.

## Git

`git status` and `git diff` are read-only context, and no destructive commands
run. Commits, pushes, issues, projects, and labels happen only when asked for. No
`Co-Authored-By` or AI attribution trailer unless asked for one.

Conventional commits where they fit: `feat|fix|docs|refactor|test|chore(scope):
summary`. A real body for anything non-trivial, wrapped at about 72 columns,
carrying the reasoning that will matter in `git log` six months later rather than
restating the diff. Call out behaviour changes, fallback paths, performance work,
and bug triggers when they motivated the change, and name the checks that
validated it. One-line commits are for genuinely tiny edits.

## Ending a session

1. Anything left over is said in the handoff, with what it would take. It is not
   filed anywhere until told where it goes.
2. `cargo fmt --all`, `cargo clippy --workspace --all-targets`, and the checks for
   what changed.
3. `git status` shows only what the task touched. Unrelated edits stay as they
   were.
4. Commit and push only if asked for.
5. Say what ran, what was checked, and what was not. A check that was not run is
   stated as not run.