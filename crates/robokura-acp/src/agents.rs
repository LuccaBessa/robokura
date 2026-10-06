//! The agents this machine has.
//!
//! An agent is what actually runs. Which model answers is the agent's own business, so
//! nothing here asks about models.

use crate::command;

/// Agents Robokura knows how to find, checked in order. A missing one is not an error,
/// because a person may have exactly one, or none.
const KNOWN: &[(&str, &str, &[&str])] = &[
    ("OpenCode", "opencode", &["acp"]),
    ("Claude Code", "claude-code-acp", &[]),
    ("Gemini CLI", "gemini", &["--experimental-acp"]),
    ("Codex CLI", "codex-acp", &[]),
    ("Goose", "goose", &["acp"]),
    ("Qwen Code", "qwen", &["--experimental-acp"]),
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found {
    pub name: &'static str,
    /// The command as it would be typed.
    pub command: String,
}

/// Looks for the known agents on this machine. This never fails: having no agent
/// installed is an ordinary state, and the answer is an empty list.
pub fn detect() -> Vec<Found> {
    KNOWN
        .iter()
        .filter_map(|(name, program, args)| {
            let path = command::resolve(program)?;
            let mut command_line = format!("\"{}\"", path.display());
            for arg in *args {
                command_line.push(' ');
                command_line.push_str(arg);
            }
            Some(Found {
                name,
                command: command_line,
            })
        })
        .collect()
}

/// An assistant with no agent cannot do anything, so creation falls back to whatever is
/// present rather than leaving the person with a dead one.
pub fn first() -> Option<String> {
    detect().first().map(|found| found.command.clone())
}

/// A short name for showing to the person. What is acting matters more than where it
/// happens to live, and a full path is neither readable nor useful in a header.
pub fn display_name(command: &str) -> String {
    for found in detect() {
        if found.command == command {
            return found.name.to_string();
        }
    }
    // Not one of the known agents, so name the agent itself.
    command
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .trim_matches('"')
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or_default()
        .trim_end_matches(".exe")
        .to_string()
}
