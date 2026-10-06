# AGENTS.md

This file defines how coding agents should behave in this repository, and what the
product is that they are changing.

## Priorities

1. Keep code simple, explicit, and maintainable.
2. Fix root causes, avoid temporary band-aids.
3. Preserve user changes, never revert unrelated edits.
4. Keep comments to a minimum.

## Workflow

1. Read this file at task start. It says what the product is and what to do next.
2. Everything runs through `cargo`. There is no task runner in this repository, so
   there is no recipe to find first and no wrapper to keep in step with the crates.
3. Before committing, always run `cargo fmt --all`, `cargo clippy --workspace
   --all-targets`, and the checks for the code that was touched. Fix any failures.
4. Anything that draws has a check that drives the same path a person does. See
   "UI verification".

## UI surfaces and crate boundaries

Robokura has one user-facing surface, the window in the `robokura` crate. There is
no second one to keep in step with it, and adding one is a product decision rather
than a mirror of the first.

The parity rule here is between the window and the two crates that must not need
one:

- `robokura-core` and `robokura-acp` compile without GPUI Kit. That is the point of
  the cut. Changing the store or a prompt must not rebuild the interface, and every
  check in those two crates must stay runnable without a window.
- Core raises plain changes. The interface turns them into a redraw. It does not
  raise the component set's own event type, because that would put a window back
  into code that has no business having one.
- When a change in the window needs something from core that core cannot answer
  without a component, the answer is not to import one. It is a finding, and it is
  reported.

## Use the component set

Every part of the interface is built from GPUI Kit components. A button is
`Button`. A list of things is `List` or `SearchableList`. A message is `Message`.
A title bar is `TitleBar`. An empty pane is `Empty`. The kit is already written,
already handles the platform, and already looks like the rest of the product.

A hand-rolled `div` with a click handler on it is not a component, and it is not
allowed. This applies to anything that behaves like a control: buttons, rows,
fields, cards, headers, bars, badges, separators, menus.

Layout is not a component, so `div` and `h_flex` and `v_flex` are for arranging
kit components, and for the surfaces between them. Where the kit and the design
disagree on a number, change the number on the kit component rather than
rebuilding the component to get the number.

Two things follow from this, and both were learned the hard way:

- **A component that exists must be looked up before one is written.** The
  components available are listed in the kit's `component` module. Read it rather
  than working from memory of what a UI usually needs.
- **Reimplementing a component inherits its bugs and loses its fixes.** A
  hand-written title bar did not get the kit's caption buttons, its platform
  handling, or its drag behaviour, and every one of those had to be rediscovered.

Where the kit genuinely cannot do something, that is a finding to raise, not a
reason to build it. Say which component is missing and what it would have to do.
The assistant's own pane is already one of these: the sidebar takes only menu
items and the dock is a full panel system, so the pane is a column of kit
components. That is a gap in the component set rather than a choice, and it is
worth saying so rather than working around it quietly.

The components are listed at `https://gpui-kit.com/component/`. Check there
before writing anything. Each page carries the import, the API, and worked
examples, and it is faster and more reliable than reading the crate source to find
out what already exists.

## GPUI threading rules

- Treat the app, window, and entity context as the UI thread unless work has
  explicitly been moved off it.
- Be extremely careful not to block the UI thread with disk I/O, network I/O,
  process start and wait, sleeps, folder removal, or CPU-heavy work. If it can
  stall a frame, assume it is forbidden on the UI thread.
- `App::spawn`, `Context::spawn`, and `AsyncWindowContext::spawn` run futures that
  are polled on the main thread. Do not put blocking or CPU-intensive work directly
  inside those futures.
- Use `cx.background_spawn(...)` or `cx.background_executor()` for blocking or
  CPU-heavy work. The foreground task starts it, awaits the result, then hops back
  through `update(...)` to apply state.
- To wrap a synchronous blocking call, move it into a background task. Do not await
  it on the foreground one, and do not introduce Tokio outside `robokura-acp`: the
  protocol client is the only thing in this repository that owns a runtime.
- Render paths must be pure state reads. No filesystem work, no store queries, no
  process work, and no expensive recomputation from a `render` method.
- Event handlers and hot paths stay thin. `on_action`, subscriptions, listeners,
  key handlers, and the polling loop start background work and return quickly rather
  than doing the slow part inline.
- Cache what is expensive to compute or load. If the interface needs it often,
  compute it in the background, store it in state, and draw from the cached copy.
  What the records say is not one of these: they are read on every redraw on
  purpose, so the screen and the store cannot drift apart.
- A background task that ends an agent must wait for the agent to have actually
  gone before anything touches what it was using. `Session::stop` is that path, and
  dropping a session is only the net under it.
- When reviewing GPUI code, ask two questions every time: **could this block?** and
  **could this run during render or another hot UI path?** If either answer is yes,
  move it off the thread.

## Commands

- Format: `cargo fmt --all`
- Format check: `cargo fmt --all -- --check`
- Lint: `cargo clippy --workspace --all-targets`
- Test: `cargo test --workspace`
- Run the app: `cargo run -p robokura`
- Checks that need a real agent installed and signed in:
  `cargo test -p robokura-core --test live -- --ignored --nocapture`

Diagnostics are off unless asked for, and go to stderr. `ROBOKURA_LOG` takes a
filter, for instance `ROBOKURA_LOG=debug`.

## Rust rules

- Do not use `unwrap()` or `expect()` in code that runs while a person is looking.
  The one in `main.rs`, where the window could not be opened, is the exception,
  because at that point there is no window to say it in.
- Checks may use them freely. A scratch folder and an `expect` that names what was
  being set up are what a check is for.
- Errors are `robokura_core::Result` and `robokura_core::Error`. Use `?` and the
  `From` impls rather than writing `map_err` by hand.
- Error text is read by a person. `Error`'s `Display` says what happened in words
  somebody can read, and keeps the underlying error for `source()` and the log.
- Ids are `String`. Do not wrap an assistant, thread, or message id in a new type
  without the user's decision, because the schema, the prompt, and the protocol all
  carry them as text.
- Do not swallow a failure with `unwrap_or_default()` where the failure means the
  work did not happen. Say it with a typed error and let the window show it.
- Keep modules focused and delete dead code instead of leaving it around.
- **Never shell out to another program.** An agent is spoken to over the protocol,
  not run as a command. There is no `Command::new` in this product.

## Code organization

- Three crates, cut by whether the code needs a window to run. Keep the cut.
- Split large files by domain. Keep source files under roughly 500 lines. The check
  files are longer and that is fine.
- Use `pub(crate)` for items shared within a crate. Apply it to fields, methods,
  and free functions.
- When code is extracted into a new file, every field and method that was reachable
  before has to stay reachable, and the source file's `use` statements get cleaned
  up.
- A type or a function exists in exactly one place. Check both files when splitting.
- `lib.rs` re-exports what the outside of a crate needs, so call sites stay short.

## Comments

Comments are kept to a minimum, and that is a ceiling rather than a target to fill.

- A comment earns its place by saying something the code does not. If deleting it
  leaves the reader with the same understanding, delete it.
- One line is the usual length. A second line is for the part that would not be
  obvious, not for the first part repeated at greater length.
- A doc comment on a public item says what the name does not. `write_body` does not
  need a comment saying that it writes a body.
- An inline comment explains why the line is there, or why it is not the obvious
  thing. Delete any that narrates: "create the thread", "store the message", "now
  render it".
- Comments are for the things that are load-bearing. The permission request answered
  without asking, the probe that is let go after twenty seconds, the retry in
  `remove_folder`, and the migration that renames the tables all carry a reason a
  reader would otherwise have to rediscover. Those keep their comments. Everything
  else is shorter.
- The voice is the one the crate already uses: plain, active, and about this product
  rather than about Rust. No metaphor, and no second person in a source comment.
- An assertion message in a check says why the claim matters, in one line. It is not
  a second copy of the check's name.

Roughly five per cent of the lines in `crates/` are comments. Treat going above that
as something to take out, not something to add to.

## Checks

Checks live in their own files, never inside a source file.

- `crates/<crate>/tests/<subject>.rs` is where a check belongs. There is no
  `#[cfg(test)] mod` in `src/`, and no `#[test]` outside a `tests/` file.
- A source file holds the code it is about. A check about the row in a list goes in
  `tests/rows.rs`, not at the bottom of the file that draws it.
- The file is named for the subject, not for the function: `tests/window.rs`,
  `tests/rows.rs`, `tests/settings.rs`, not `tests/when.rs`.
- A check lives in the crate that owns the subject. A check that needs a window is in
  `robokura`, and everything in `robokura-core` and `robokura-acp` has to stay
  runnable without one.
- A check may only reach what the crate makes public. If a check needs a field or a
  function that is not reachable, that is a finding to report rather than something to
  solve by widening the whole crate to `pub`.
- The checks that start a real agent are in `tests/live.rs` and are `#[ignore]`d.
  `cargo test` must never start one.

## Features and dependencies

- Every dependency version lives in the root `Cargo.toml` `[workspace.dependencies]`.
  Subcrate `Cargo.toml` files use `{ workspace = true }`. Never hardcode a version
  in a subcrate.
- The only feature in use is `gpui-kit`'s `test-support`, and it is a
  dev-dependency of `robokura`, so the headless window is not compiled into what a
  person runs. Keep it that way. A test-only helper belongs in `dev-dependencies`.
- New features use `dep:crate_name` syntax for optional dependencies, and
  `#[cfg(feature = "...")]` on the module, on its `use`, and on every item that
  names its types.
- **A feature must never switch off one of the parts that are not negotiable.** No
  build of this product may be a build where the credential boundary, the
  isolation rule, or the rule that a person approves an irreversible action is
  optional. A feature that seems to need that is a design question, not a feature,
  and it goes to the user.
- `[profile.dev]` strips debug info from dependencies because the GPUI dependency
  tree is large. Do not undo that to make one build faster.

## Common mistakes to avoid

- **Putting a window in `robokura-core` or `robokura-acp`.** Changing the store or
  a prompt must not rebuild the interface, and a store change must stay checkable
  without one. If the change seems to need a component, it is in the wrong crate.
- **Walking a thread to find its newest message.** The list reads every row on
  every redraw. `Store::previews` answers for all threads in one read and returns a
  cut of the message. A loop over the threads is the slow version of something
  already answered.
- **Truncating twice.** A preview is already cut by `PREVIEW_CHARS`. Adding an
  ellipsis in the row on top of it leaves the row showing nothing but the ellipsis.
  Choose one layer, and put a check on the helper.
- **Caching what the store should answer.** Records are read on every redraw so
  that what is on screen and what is stored cannot drift apart. Cache what the
  store does not already answer cheaply, not the records.
- **Replacing the wording for a folder that will not go.** `Error::AgentStillRunning`
  says an agent is still holding its own folder, because somebody removing an
  assistant has no file in mind. The retry in `remove_folder` exists because an
  agent that has just been asked to stop takes a moment to let go. Do not shorten
  the wait, and do not surface the operating system's complaint instead.
- **A check that starts a real agent without `#[ignore]`.** Those checks live in
  `tests/live.rs`, need an agent installed and signed in, and are slow. `cargo test`
  must never start one.
- **Naming a check after the function it calls.** Checks are named for what has to
  be true, so the name survives a rename.
- **Writing and publishing in one path.** They are separate steps and the second
  one is asked for. There is no call that does both.

## Conventions

- Prose follows the same rules as the rest of the project: active voice, short
  sentences, no em-dashes, no metaphors, no second person. The top-level
  `README.md` is customer-facing and may address the reader directly.
- The product is described on its own terms. Do not build the argument by
  contrasting it with another product, and do not name other products.
- No due dates anywhere. Progress is measured by working steps.
- No market, pricing, money, or competitive content in the README or anything said
  about the product. A run is described by what it did and how long it took.
  Nothing else.

## Git rules

- Treat `git status` and `git diff` as read-only context.
- Do not run destructive git commands.
- Do not amend commits unless explicitly asked.
- Only create commits when the user asks.
- Do not create GitHub issues, projects, or labels unless the user explicitly asks.
  No tracker is to be added in their place.
- There is no `Co-Authored-By` or AI attribution trailer unless the user asks for
  one.

## Commit messages

- Prefer conventional commits when they fit:
  `feat|fix|docs|refactor|test|chore(scope): summary`.
- Use a real commit body for any non-trivial change. One-line commits are for
  genuinely tiny edits only.
- Structure commit messages like this:
  1. Subject line: concise, imperative, and specific about the user-visible or
     architectural change.
  2. Blank line.
  3. Body: short paragraphs or bullets explaining why the change was needed, what
     changed, and any important constraints, follow-ups, or migrations.
- Wrap commit message text. Keep the subject short, and wrap body lines to roughly
  72 columns so `git log` and terminal tools stay readable.
- The body should capture the reasoning that will matter in `git log` six months
  later, not just restate the diff.
- Call out behavior changes, fallback paths, performance work, or bug triggers
  explicitly when they motivated the change.
- If validation was important, mention the key checks in the body instead of making
  reviewers guess.
- Avoid useless subjects like `fix stuff`, `updates`, `wip`, or `misc cleanup`.

## UI verification

- Anything that draws gets a check that goes through the same path a person does.
  `crates/robokura/tests/window.rs` opens a real window on a headless context over a
  store of its own, so a control, the bar's toggle, and a question asked are all
  covered without a person present.
- A check is written as something a person can do and see, not as a statement about
  the code. Name the thing that has to be true.
- Run `cargo run -p robokura` for what a check cannot see: focus, the keyboard, drag,
  the platform's own window furniture, and how a pane sits against the bar.
- Look at the window rather than assuming. A check that finds an element is not the
  same as the layout being right, and the bar's reserved width is worked out from
  the component's own published height rather than written down, so it is worth
  seeing once.

## Changelog

No changelog file is tracked and none is generated. Release notes are written when a
release is cut.

- Notes describe what changed for a person, in the voice of the README: what can now
  be done, and what still cannot. Not the diff, and not the commit list.
- What is deliberately not built is part of the notes. This product says plainly
  what a version does not do.

## Project structure

| Crate | Description |
|---|---|
| `robokura-acp` | Speaks the Agent Client Protocol. Knows nothing about assistants or threads, and owns the only Tokio runtime |
| `robokura-core` | The three records, the store, the prompt builder, one session per assistant. No interface code, ever |
| `robokura` | The window, the panes, `main`. The only crate with an interface |

An arrow points at what is depended on.

```
robokura  ->  robokura-core  ->  robokura-acp
```

## Ending a session

1. Anything left over is said in the handoff, with what it would take. It is not
   filed anywhere until the user says where it goes.
2. Run `cargo fmt --all`, `cargo clippy --workspace --all-targets`, and the checks
   for what changed.
3. `git status` shows only what the task touched. Unrelated edits stay as they were.
4. Commit and push only if the user asked for it. Nothing is pushed on a hunch.
5. Say what ran, what was checked, and what was not. A check that was not run is
   stated as not run.
