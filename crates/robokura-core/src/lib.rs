//! Personal assistants that run on your own computer.
//!
//! This crate is everything that is not a window: the three records, where they are
//! kept, what an assistant is sent, and the agents those assistants run on. It has
//! no interface code and never will, because the day it does it stops being
//! checkable without one.

pub mod domain;
pub mod error;
pub mod paths;
pub mod prompt;
pub mod run;
pub mod store;

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

pub use domain::{Assistant, ConfigOption, Kind, Message, Preview, Settings, Thread};
pub use error::{Error, Result};

use run::{Outcome, Run};
use store::Store;

pub const NEW_ASSISTANT_NAME: &str = "New assistant";

/// Empty rather than filled in. A title says what an assistant is for, and a
/// placeholder would be a claim about it that nobody made.
pub const NEW_ASSISTANT_TITLE: &str = "";

pub const NEW_ASSISTANT_DESCRIPTION: &str = "A newly created assistant.";

/// Everything the window asks, and the one thing it holds.
pub struct Core {
    root: PathBuf,
    store: Store,
    /// One running agent per assistant, started when that assistant is first spoken
    /// to.
    runs: HashMap<String, Run>,
    /// Problems not tied to a running agent, such as one that would not start.
    problems: HashMap<String, String>,
    probe: Option<Instant>,
}

/// Assistant ids are thirty-two hexadecimal characters, so a probe can never collide
/// with one the person made.
const PROBE: &str = "__probe";

/// Holding a probe's run open for ever would keep the window waking for an agent that
/// has gone quiet.
const PROBE_WAIT: Duration = Duration::from_secs(20);

impl Core {
    pub fn open() -> Result<Self> {
        let root = paths::root().ok_or(Error::NowhereToKeepFiles)?;
        Self::open_at(root)
    }

    /// The folder is a parameter rather than something found from the environment,
    /// which is what lets the checks run against a scratch folder.
    pub fn open_at(root: PathBuf) -> Result<Self> {
        Ok(Self {
            store: Store::open(&paths::db_file(&root))?,
            root,
            runs: HashMap::new(),
            problems: HashMap::new(),
            probe: None,
        })
    }

    // Assistants.

    /// Read on every redraw rather than held, so what is on screen and what is stored
    /// cannot drift apart.
    pub fn settings(&self) -> Result<Settings> {
        self.store.settings()
    }

    pub fn save_settings(&mut self, settings: &Settings) -> Result<()> {
        self.store.save_settings(settings)
    }

    /// Started rather than waited for, because an agent that blocked here would freeze
    /// the window for as long as it took to come up.
    pub fn probe(&mut self, agent: &str) {
        let agent = agent.trim();
        if agent.is_empty() || self.probe.is_some() {
            return;
        }

        let home = paths::assistant_home(&self.root, PROBE);
        if std::fs::create_dir_all(&home).is_err() {
            return;
        }

        let settings = self.store.settings().unwrap_or_default();
        let asked = Assistant {
            id: PROBE.to_string(),
            name: PROBE.to_string(),
            title: String::new(),
            description: String::new(),
            runs_on: agent.to_string(),
            created_at: 0,
        };

        if let Ok(run) = Run::start(&asked, home, &settings) {
            self.probe = Some(Instant::now());
            self.runs.insert(PROBE.to_string(), run);
        }
    }

    pub fn probing(&self) -> bool {
        self.probe.is_some()
    }

    pub fn assistants(&self) -> Result<Vec<Assistant>> {
        self.store.assistants()
    }

    pub fn assistant(&self, id: &str) -> Result<Option<Assistant>> {
        self.store.assistant(id)
    }

    /// The thread and everything in it are left alone, and a running agent is not told:
    /// the purpose it was given when it started does not change mid-turn.
    pub fn update_assistant(
        &mut self,
        id: &str,
        name: impl Into<String>,
        title: impl Into<String>,
        description: impl Into<String>,
    ) -> Result<Assistant> {
        let mut assistant = self
            .store
            .assistant(id)?
            .ok_or_else(|| Error::NotFound("that assistant".to_string()))?;
        assistant.name = name.into();
        assistant.title = title.into();
        assistant.description = description.into();

        if !self.store.update_assistant(&assistant)? {
            return Err(Error::NotFound("that assistant".to_string()));
        }
        Ok(assistant)
    }

    /// The folder is made before the record, so an assistant that cannot be given
    /// somewhere to run is never listed.
    pub fn create_assistant(
        &mut self,
        name: impl Into<String>,
        title: impl Into<String>,
        description: impl Into<String>,
    ) -> Result<Assistant> {
        let assistant = Assistant::new(name, title, description, self.runs_on());
        std::fs::create_dir_all(paths::assistant_home(&self.root, &assistant.id))?;
        self.store.add_assistant(&assistant)?;
        Ok(assistant)
    }

    /// Found when an assistant is made rather than kept, so an agent installed after
    /// the settings were last opened is still used.
    fn runs_on(&self) -> String {
        let chosen = self
            .store
            .settings()
            .map(|settings| settings.runs_on)
            .unwrap_or_default();

        if chosen.trim().is_empty() {
            robokura_acp::agents::first().unwrap_or_default()
        } else {
            chosen
        }
    }

    /// Asks for nothing first: the three fields are all changed on the assistant's own
    /// pane, so asking here would be a second place to type them.
    pub fn create_default_assistant(&mut self) -> Result<Assistant> {
        self.create_assistant(
            NEW_ASSISTANT_NAME,
            NEW_ASSISTANT_TITLE,
            NEW_ASSISTANT_DESCRIPTION,
        )
    }

    pub fn delete_assistant(&mut self, id: &str) -> Result<()> {
        // The agent is ended, and waited for, before the folder is touched. It was
        // started inside that folder, and a running process holds it open.
        self.stop_agent(id);

        if self.store.delete_assistant(id)? {
            let home = paths::assistant_home(&self.root, id);
            if home.exists() {
                remove_folder(&home)?;
            }
        }
        Ok(())
    }

    /// Taking the run out of the map is not enough on its own. Dropping a session only
    /// asks for the shutdown and returns.
    fn stop_agent(&mut self, id: &str) {
        self.problems.remove(id);
        if let Some(mut run) = self.runs.remove(id) {
            run.stop();
        }
    }

    // A thread.

    pub fn messages(&self, assistant_id: &str) -> Result<Vec<Message>> {
        match self.store.thread_for(assistant_id)? {
            Some(thread) => self.store.messages(&thread.id),
            None => Ok(Vec::new()),
        }
    }

    pub fn previews(&self) -> Result<Vec<Preview>> {
        self.store.previews()
    }

    pub fn working(&self, assistant_id: &str) -> bool {
        self.runs.get(assistant_id).map(Run::busy).unwrap_or(false)
    }

    /// An assistant that is silent while it thinks looks the same as one that has
    /// stopped, so the pane says so.
    pub fn thinking(&self, assistant_id: &str) -> bool {
        self.runs
            .get(assistant_id)
            .map(Run::thinking)
            .unwrap_or(false)
    }

    /// The window reads this to decide how often to look.
    pub fn busy(&self) -> bool {
        self.runs.values().any(Run::busy)
    }

    pub fn problem(&self, assistant_id: &str) -> Option<&str> {
        self.problems.get(assistant_id).map(String::as_str)
    }

    // A turn.

    /// The message is written before the agent is asked, so the question is on screen
    /// even if the agent never manages to answer.
    pub fn send(&mut self, assistant_id: &str, text: &str) -> Result<()> {
        let text = text.trim();
        if text.is_empty() {
            return Ok(());
        }

        let assistant = self
            .store
            .assistant(assistant_id)?
            .ok_or_else(|| Error::NotFound("that assistant".to_string()))?;
        let thread = self
            .store
            .thread_for(assistant_id)?
            .ok_or_else(|| Error::NotFound("that assistant's thread".to_string()))?;

        // An agent that has just started holds no history, so the first message
        // carries everything already said. What is already said is read before the
        // new message is stored, so the question is not in the history as well as at
        // the end of the prompt.
        let starting = !self.runs.contains_key(assistant_id);
        let out = if starting {
            let history = self.store.messages(&thread.id)?;
            prompt::opening(&assistant, &history, text)
        } else {
            prompt::follow_up(text)
        };

        let seq = self.store.next_seq(&thread.id)?;
        self.store
            .add_message(&Message::person(&thread.id, seq, text))?;

        if starting {
            match self.start_run(&assistant) {
                Ok(run) => {
                    self.runs.insert(assistant_id.to_string(), run);
                }
                Err(problem) => {
                    self.problems.insert(assistant_id.to_string(), problem);
                    return Ok(());
                }
            }
        }

        // The reply row goes in before the agent begins, so a turn that is cut short
        // leaves a record of how far it got. A message arriving while a turn is
        // running is held until that turn ends rather than started on top of it.
        if self.holding(assistant_id) {
            if let Some(run) = self.runs.get_mut(assistant_id) {
                run.defer(out);
            }
            return Ok(());
        }

        let seq = self.store.next_seq(&thread.id)?;
        let reply = Message::from_assistant(&thread.id, seq, &assistant, Kind::Text, "", false);
        self.store.add_message(&reply)?;
        self.problems.remove(assistant_id);

        if let Some(run) = self.runs.get_mut(assistant_id) {
            run.begin(&thread.id, &reply.id);
            run.send(out);
        }
        Ok(())
    }

    /// Asks the agent to stop. A stopped turn is not a failure.
    pub fn stop(&mut self, assistant_id: &str) {
        if let Some(run) = self.runs.get(assistant_id) {
            run.cancel();
        }
    }

    /// Reads from every running agent and writes down what changed.
    pub fn pump(&mut self) {
        if let Some(started) = self.probe
            && started.elapsed() > PROBE_WAIT
        {
            self.probe = None;
            self.runs.remove(PROBE);
        }

        let running: Vec<String> = self.runs.keys().cloned().collect();
        for assistant_id in running {
            let Some(outcome) = self.runs.get_mut(&assistant_id).map(Run::poll) else {
                continue;
            };
            self.apply(&assistant_id, outcome);
        }
    }

    fn apply(&mut self, assistant_id: &str, outcome: Outcome) {
        // A probe is not an assistant and has no thread. It is here for one answer
        // only, and is let go as soon as it has given it.
        if assistant_id == PROBE {
            if !outcome.selectors.is_empty() {
                let command = self
                    .runs
                    .get(assistant_id)
                    .map(|run| run.command().to_string())
                    .unwrap_or_default();
                if let Ok(mut settings) = self.store.settings() {
                    settings.offered.insert(command, outcome.selectors);
                    let _ = self.store.save_settings(&settings);
                }
                self.probe = None;
                self.runs.remove(assistant_id);
            }
            return;
        }

        let Some(thread_id) = self
            .runs
            .get(assistant_id)
            .map(|run| run.thread_id().to_string())
        else {
            return;
        };
        let Some(assistant) = self.store.assistant(assistant_id).unwrap_or(None) else {
            return;
        };

        for note in outcome.notes {
            if let Ok(seq) = self.store.next_seq(&thread_id) {
                let _ = self.store.add_message(&Message::from_assistant(
                    &thread_id,
                    seq,
                    &assistant,
                    Kind::Note,
                    note,
                    true,
                ));
            }
        }

        if let Some(reason) = &outcome.finished {
            if reason != "done" {
                if let Ok(seq) = self.store.next_seq(&thread_id) {
                    let _ = self.store.add_message(&Message::from_assistant(
                        &thread_id,
                        seq,
                        &assistant,
                        Kind::Note,
                        format!("Turn {reason}."),
                        true,
                    ));
                }
            }
        }

        if let Some(problem) = outcome.failed.clone() {
            self.problems
                .insert(assistant_id.to_string(), problem.clone());
            if let Ok(seq) = self.store.next_seq(&thread_id) {
                let _ = self.store.add_message(&Message::from_assistant(
                    &thread_id,
                    seq,
                    &assistant,
                    Kind::Note,
                    problem,
                    true,
                ));
            }
        }

        // The reply is closed only when the agent finished it. A turn that was
        // stopped, or an agent that went away, keeps its row unfinished so it reads
        // as cut off rather than complete.
        if let Some(text) = outcome.held {
            let id = self
                .runs
                .get(assistant_id)
                .and_then(|run| run.reply_id())
                .map(str::to_string);
            if let Some(id) = id {
                let _ = self.store.write_body(&id, &text, outcome.reply_closed);
            }
        }

        let turn_over = outcome.finished.is_some() || outcome.failed.is_some() || outcome.ended;
        if turn_over && let Some(run) = self.runs.get_mut(assistant_id) {
            run.finish();
        }

        // A message that waited for this turn is not sent if the agent has gone,
        // because the person's message is already on record.
        if outcome.ended {
            self.runs.remove(assistant_id);
        } else if let Some(waiting) = outcome.resume {
            if let Ok(seq) = self.store.next_seq(&thread_id) {
                let reply =
                    Message::from_assistant(&thread_id, seq, &assistant, Kind::Text, "", false);
                if self.store.add_message(&reply).is_ok()
                    && let Some(run) = self.runs.get_mut(assistant_id)
                {
                    run.begin(&thread_id, &reply.id);
                    run.send(waiting);
                }
            }
        }
    }

    /// Writes back anything still held, because the window is closing.
    pub fn flush(&mut self) {
        for assistant_id in self.runs.keys().cloned().collect::<Vec<_>>() {
            let Some(run) = self.runs.get_mut(&assistant_id) else {
                continue;
            };
            let held = run.take_held();
            let Some(text) = held else {
                continue;
            };
            let id = run.reply_id().map(str::to_string);
            if let Some(id) = id {
                let _ = self.store.write_body(&id, &text, false);
            }
        }
    }

    fn start_run(&self, assistant: &Assistant) -> std::result::Result<Run, String> {
        let home = paths::assistant_home(&self.root, &assistant.id);
        std::fs::create_dir_all(&home)
            .map_err(|error| format!("The agent's own folder could not be made: {error}"))?;

        // Read here rather than passed in, so that starting an agent and the settings
        // that shaped it cannot be two reads of two different moments.
        let settings = self.store.settings().unwrap_or_default();
        Run::start(assistant, home, &settings)
    }

    fn holding(&self, assistant_id: &str) -> bool {
        self.runs.get(assistant_id).map(Run::busy).unwrap_or(false)
    }
}

/// An agent that has only just been killed still takes a moment to let go of the
/// folder it was started in, so the first attempt can lose to it.
const REMOVE_ATTEMPTS: usize = 10;
const REMOVE_WAIT: Duration = Duration::from_millis(50);

/// The only thing that can be inside an assistant's folder is its own agent, and that
/// has already been asked to stop, so a folder that will not go is an agent that has
/// not.
fn remove_folder(home: &std::path::Path) -> Result<()> {
    for attempt in 0..REMOVE_ATTEMPTS {
        match std::fs::remove_dir_all(home) {
            Ok(()) => return Ok(()),
            Err(error) if attempt + 1 < REMOVE_ATTEMPTS => {
                let _ = error;
                std::thread::sleep(REMOVE_WAIT);
            }
            Err(error) => return Err(Error::AgentStillRunning(error)),
        }
    }
    Err(Error::AgentStillRunning(std::io::Error::other(
        "the folder did not go",
    )))
}
