//! What the agent is sent.
//!
//! An agent knows nothing about being the assistant called Letters, so what that
//! assistant is for goes with every message. The thread goes with the first one only,
//! because after that the running agent already holds it.

use crate::domain::{Assistant, Message};

pub fn purpose(assistant: &Assistant) -> String {
    let mut text = format!("You are {}.", assistant.name.trim());

    let title = assistant.title.trim();
    if !title.is_empty() {
        text.push_str(&format!(" Your role is {title}."));
    }

    let description = assistant.description.trim();
    if !description.is_empty() {
        text.push(' ');
        text.push_str(description);
    }

    text
}

pub fn opening(assistant: &Assistant, history: &[Message], new: &str) -> String {
    let mut text = purpose(assistant);

    // A reply that was cut off is left out rather than shown as something the agent
    // said. It is still on screen, marked as unfinished.
    let said: Vec<&Message> = history
        .iter()
        .filter(|message| message.complete && message.kind == crate::domain::Kind::Text)
        .collect();

    if !said.is_empty() {
        text.push_str("\n\nThis conversation so far:\n");
        for message in said {
            let who = if message.is_person() {
                "Person".to_string()
            } else {
                message.author_name.clone()
            };
            text.push_str(&format!("{who}: {}\n", message.body.trim()));
        }
    }

    text.push_str(&format!("\nPerson: {}", new.trim()));
    text
}

/// A later message. The agent is still running and already holds the rest.
pub fn follow_up(new: &str) -> String {
    new.trim().to_string()
}
