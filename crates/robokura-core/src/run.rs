//! One assistant's live agent.
//!
//! A run holds the connection to the agent it runs on and the text of the reply being
//! written. It decides nothing about what is kept: it reports what happened, and
//! `Core` writes it down.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use robokura_acp::{Event, Session, Start};

use crate::domain::{Assistant, ConfigOption, Settings};

/// The text arrives faster than a window should be redrawn.
const WRITE_EVERY: Duration = Duration::from_millis(200);

/// What one round of reading from an agent produced.
#[derive(Debug, Default)]
pub struct Outcome {
    /// Lines of progress to store alongside the messages.
    pub notes: Vec<String>,
    /// Whether the agent has said nothing yet, so the pane can say so rather than
    /// looking stuck.
    pub thinking: bool,
    /// Text arrived since the last write, or nothing.
    pub held: Option<String>,
    pub finished: Option<String>,
    pub failed: Option<String>,
    /// The agent ended, expected or not.
    pub ended: bool,
    /// A turn the person stopped, or an agent that went away, is not this: it is left
    /// marked unfinished so it reads as what it was.
    pub reply_closed: bool,
    /// A message held back while a turn was in flight, which can go now.
    pub resume: Option<String>,
    /// Belongs to the machine rather than to this assistant, so it goes out to the
    /// window rather than being kept here.
    pub selectors: Vec<ConfigOption>,
}

pub struct Run {
    session: Session,
    command: String,
    thread_id: String,
    /// The reply being written, absent between turns.
    reply: Option<String>,
    /// The whole reply rather than only what has arrived since the last write, because
    /// a turn ends on an event that brings no text with it.
    body: String,
    written: usize,
    /// The last line for each tool call, so a call that reports its status repeatedly
    /// is one line rather than one line per report.
    tools: HashMap<String, String>,
    spoken: bool,
    waiting: Option<String>,
    last_write: Instant,
}

impl Run {
    pub fn start(
        assistant: &Assistant,
        home: std::path::PathBuf,
        settings: &Settings,
    ) -> std::result::Result<Self, String> {
        let command = if assistant.runs_on.trim().is_empty() {
            robokura_acp::agents::first().unwrap_or_default()
        } else {
            assistant.runs_on.clone()
        };
        if command.trim().is_empty() {
            return Err("No agent was found on this machine.".to_string());
        }

        let session = Session::start(Start {
            command: command.clone(),
            cwd: home,
            config: settings
                .chosen
                .iter()
                .map(|(id, value)| (id.clone(), value.clone()))
                .collect(),
        })?;

        Ok(Self {
            session,
            command,
            thread_id: String::new(),
            reply: None,
            body: String::new(),
            written: 0,
            tools: HashMap::new(),
            spoken: false,
            waiting: None,
            last_write: Instant::now(),
        })
    }

    pub fn thread_id(&self) -> &str {
        &self.thread_id
    }

    /// So what the agent said about itself can be filed against the agent that said
    /// it rather than against one assistant.
    pub fn command(&self) -> &str {
        &self.command
    }

    /// Begins a reply, so a turn that is cut short still has a row to write to.
    pub fn begin(&mut self, thread_id: &str, reply_id: &str) {
        self.thread_id = thread_id.to_string();
        self.reply = Some(reply_id.to_string());
        self.body.clear();
        self.written = 0;
        self.tools.clear();
        self.spoken = false;
        self.last_write = Instant::now();
    }

    /// A message arriving while a turn is running waits for it rather than racing it.
    pub fn busy(&self) -> bool {
        self.reply.is_some()
    }

    pub fn thinking(&self) -> bool {
        self.reply.is_some() && !self.spoken
    }

    pub fn reply_id(&self) -> Option<&str> {
        self.reply.as_deref()
    }

    pub fn send(&self, prompt: String) {
        self.session.send_prompt(prompt);
    }

    /// Sending straight away would either be refused or be read as part of the turn
    /// already running, so the message waits and the turn is asked to end.
    pub fn defer(&mut self, prompt: String) {
        self.waiting = Some(prompt);
        self.session.cancel();
    }

    pub fn cancel(&self) {
        self.session.cancel();
    }

    pub fn stop(&mut self) {
        self.session.stop();
    }

    pub fn poll(&mut self) -> Outcome {
        let mut outcome = Outcome::default();
        let mut turn_over = false;

        while let Some(event) = self.session.next_event() {
            match event {
                Event::Ready { .. } => {}

                // Taken once and offered on. Not stored as a message: a thread full
                // of lines about models would bury the conversation it belongs to.
                Event::Selectors(selectors) => {
                    outcome.selectors = selectors
                        .into_iter()
                        .map(|selector| ConfigOption {
                            id: selector.id,
                            name: selector.name,
                            category: selector.category,
                            values: selector.values,
                            current: selector.current,
                        })
                        .collect();
                }

                Event::Text(text) => {
                    self.body.push_str(&text);
                    self.spoken = true;
                }

                // The agent's own reasoning is not stored. It arrives in pieces as
                // often as the agent breathes, so keeping each piece would put a row
                // in the thread for every few words of thinking.
                Event::Thought(_) => {}

                Event::ToolStarted { id, title } => {
                    self.note_tool(&mut outcome, id, label("working", &title));
                }

                Event::ToolStatus {
                    id, title, status, ..
                } => {
                    self.note_tool(&mut outcome, id, label(&status, &title));
                }

                Event::NeedsPermission { id, choices, .. } => {
                    // Answered here rather than put to the person, because this
                    // version has nothing to show a question in and nowhere for the
                    // answer to come from. An agent waiting on an answer nobody can
                    // give stops for good, which is worse than one that acts.
                    //
                    // Said in the thread either way, because an agent that acted
                    // without asking and one that asked and was allowed look
                    // identical afterwards otherwise.
                    let answer = choices
                        .iter()
                        .find(|choice| choice.allows)
                        .map(|choice| choice.option_id.clone());
                    self.session.answer_permission(id, answer);
                    outcome.notes.push("Allowed without asking.".to_string());
                }

                Event::Finished { reason } => {
                    turn_over = true;
                    // A turn that was asked to stop ended on purpose, so what it was
                    // writing is left unfinished. Everything it already did stays done.
                    outcome.reply_closed = reason != "cancelled";
                    outcome.finished = Some(reason);
                }

                Event::Failed(problem) => {
                    turn_over = true;
                    outcome.reply_closed = true;
                    outcome.failed = Some(problem);
                }

                Event::Disconnected => {
                    turn_over = true;
                    outcome.ended = true;
                    outcome.reply_closed = false;
                }
            }
        }

        // Written on the interval while the reply is arriving, and at once when the
        // turn ends even if no new words came with it, because the row has to be
        // closed with everything the reply did say.
        let due = turn_over
            || (self.body.len() > self.written && self.last_write.elapsed() >= WRITE_EVERY);
        if due {
            outcome.held = Some(self.body.clone());
            self.written = self.body.len();
            self.last_write = Instant::now();
        }

        if turn_over {
            // Handed back rather than sent here, because starting the next reply
            // needs a row to write to and only the caller knows where rows come from.
            outcome.resume = self.waiting.take();
        } else {
            outcome.thinking = !self.spoken;
        }

        outcome
    }

    /// A call reports its status as it goes, so without this a single tool call puts
    /// a line in the thread every time it reports.
    fn note_tool(&mut self, outcome: &mut Outcome, id: String, line: String) {
        if self.tools.get(&id) == Some(&line) {
            return;
        }
        self.tools.insert(id, line.clone());
        outcome.notes.push(line);
    }

    /// Writes back anything not yet written, because the application is closing.
    pub fn take_held(&mut self) -> Option<String> {
        if self.body.len() <= self.written {
            return None;
        }
        self.written = self.body.len();
        self.last_write = Instant::now();
        Some(self.body.clone())
    }

    pub fn finish(&mut self) {
        self.reply = None;
        self.body.clear();
        self.written = 0;
    }
}

fn label(status: &str, title: &str) -> String {
    let title = title.trim();
    if title.is_empty() {
        format!("· {status}")
    } else {
        format!("· {title} {status}")
    }
}
