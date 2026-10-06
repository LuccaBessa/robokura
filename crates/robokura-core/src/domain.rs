//! The things this application keeps.
//!
//! An assistant, the thread it holds with the person, the messages in it, and what the
//! person has configured for the machine rather than for any one of them. No other
//! record exists, and nothing here knows about a window.
//!
//! An assistant is what a person makes and talks to. An agent is the thing it runs on,
//! which the protocol treats as an agent in its own right and which this product does
//! not own.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// The title and the description are the person's words for what this assistant is
/// for. They are sent with every message the agent receives, so they are part of how it
/// works rather than a label kept beside it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Assistant {
    pub id: String,
    pub name: String,
    pub title: String,
    pub description: String,
    /// The agent this assistant runs on, as the command that starts it. Which model
    /// answers is the agent's own business, so changing the model never changes what
    /// is acting.
    pub runs_on: String,
    pub created_at: i64,
}

impl Assistant {
    pub fn new(
        name: impl Into<String>,
        title: impl Into<String>,
        description: impl Into<String>,
        runs_on: impl Into<String>,
    ) -> Self {
        Self {
            id: new_id(),
            name: name.into(),
            title: title.into(),
            description: description.into(),
            runs_on: runs_on.into(),
            created_at: now_seconds(),
        }
    }
}

/// One assistant has exactly one thread, which the database enforces.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Thread {
    pub id: String,
    pub assistant_id: String,
    pub created_at: i64,
    pub updated_at: i64,
}

/// A note is a line of progress: something the agent did, or something it was thinking.
/// Notes are what makes the work visible while it happens.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Text,
    Note,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Text => "text",
            Kind::Note => "note",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub thread_id: String,
    /// Assigned by this application, never by a clock, so two messages cannot land on
    /// the same place.
    pub seq: i64,
    pub kind: Kind,
    /// Absent when the person wrote it.
    pub assistant_id: Option<String>,
    /// Copied here rather than read back from the assistant, so renaming one does not
    /// rewrite what it said and deleting one does not turn its past messages anonymous.
    pub author_name: String,
    pub body: String,
    pub created_at: i64,
    pub updated_at: i64,
    /// False while a reply is still arriving. A reply that was cut off is kept as one
    /// that was cut off rather than dropped.
    pub complete: bool,
}

impl Message {
    pub fn person(thread_id: &str, seq: i64, body: impl Into<String>) -> Self {
        Self {
            id: new_id(),
            thread_id: thread_id.to_string(),
            seq,
            kind: Kind::Text,
            assistant_id: None,
            author_name: "You".to_string(),
            body: body.into(),
            created_at: now_seconds(),
            updated_at: now_seconds(),
            complete: true,
        }
    }

    pub fn from_assistant(
        thread_id: &str,
        seq: i64,
        assistant: &Assistant,
        kind: Kind,
        body: impl Into<String>,
        complete: bool,
    ) -> Self {
        Self {
            id: new_id(),
            thread_id: thread_id.to_string(),
            seq,
            kind,
            assistant_id: Some(assistant.id.clone()),
            author_name: assistant.name.clone(),
            body: body.into(),
            created_at: now_seconds(),
            updated_at: now_seconds(),
            complete,
        }
    }

    /// True when the person wrote it, which is also true when no assistant is behind it.
    pub fn is_person(&self) -> bool {
        self.assistant_id.is_none()
    }
}

/// What one assistant's row says about its thread. The body is a cut of the message
/// rather than the whole of it, because the list reads this for every assistant on
/// every redraw.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Preview {
    pub assistant_id: String,
    pub body: String,
    /// The moment the message was stored rather than the moment it was last written to,
    /// so a reply still arriving does not push the time along as its text grows.
    pub at: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    /// Empty means the first agent found on this machine. An assistant already made
    /// keeps the agent it was made with: a setting that answered here would be changing
    /// which software is acting under an assistant that is mid-conversation.
    #[serde(default)]
    pub runs_on: String,

    /// Keyed by the agent's own word for the selector rather than by what we call it: a
    /// model called `model` in one agent is a mode in another.
    #[serde(default)]
    pub chosen: BTreeMap<String, String>,

    /// What each agent last said it could be set to, keyed by the agent's command, so
    /// the choices can be shown before any assistant has been spoken to.
    #[serde(default)]
    pub offered: BTreeMap<String, Vec<ConfigOption>>,

    /// `light` or `dark`, as the person has put it. Empty is the shipped answer and
    /// means the machine's own, which is also what a window nobody has opened the
    /// settings in is drawn in.
    #[serde(default)]
    pub mode: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigOption {
    pub id: String,
    pub name: String,
    /// As the agent says it. Unknown values are kept rather than dropped, because an
    /// agent is free to add its own.
    #[serde(default)]
    pub category: String,
    pub values: Vec<(String, String)>,
    /// The agent's own value rather than ours.
    #[serde(default)]
    pub current: String,
}

fn new_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

fn now_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or_default()
}
