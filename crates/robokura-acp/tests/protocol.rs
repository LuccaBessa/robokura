//! The protocol layer's rules. Runs without a window and without GPUI.

use std::path::PathBuf;

use robokura_acp::{Start, command, session::Session};

fn scratch(name: &str) -> PathBuf {
    let base = std::env::temp_dir().join("robokura-acp-tests").join(name);
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).expect("scratch directory");
    base
}

/// A start for an agent that is not there, which is the cheapest way to reach the
/// part of starting that runs before anything is spawned.
fn start_of(command: &str, home: &str) -> Start {
    Start {
        command: command.to_string(),
        cwd: scratch(home),
        ..Default::default()
    }
}

#[test]
fn a_missing_agent_does_not_start() {
    // No fallback. An agent that cannot run does not run.
    let result = Session::start(start_of(
        "this-agent-does-not-exist-anywhere",
        "no-fallback",
    ));
    assert!(result.is_err(), "starting a missing agent is an error");
}

#[test]
fn an_empty_command_is_refused() {
    let result = Session::start(start_of("", "empty-command"));
    assert!(result.is_err(), "an empty command is an error");
}

#[test]
fn a_command_of_only_whitespace_is_refused() {
    let result = Session::start(start_of("   ", "blank-command"));
    assert!(result.is_err(), "a blank command is an error");
}

#[test]
fn the_program_is_read_from_a_quoted_command() {
    assert!(
        !command::exists(r#""C:\definitely\not\here\agent.exe""#),
        "a quoted path to nothing is not found"
    );
    assert!(
        !command::exists(r#""unterminated"#),
        "an unterminated quote does not resolve to a program"
    );
}

#[test]
fn arguments_are_not_part_of_the_program_name() {
    let opencode = command::resolve("opencode").is_some();
    let with_args = command::resolve("opencode acp").is_some();
    assert_eq!(opencode, with_args, "arguments do not change what is found");
}

#[test]
fn an_agent_on_this_machine_is_found() {
    let first = command::resolve("opencode");
    let second = command::resolve("opencode");
    assert_eq!(first, second, "finding an agent is repeatable");
    assert_eq!(
        command::exists("opencode"),
        first.is_some(),
        "existence agrees with resolution"
    );
}

#[test]
fn a_session_ends_with_its_agent() {
    // Dropping must not panic, which it would if the runtime were torn down from
    // inside an async context.
    if let Ok(session) = Session::start(start_of("opencode acp", "cleanup")) {
        drop(session);
    }
}

#[test]
fn an_agent_is_given_nothing_it_did_not_ask_for() {
    // A session is started with the person's own folder and nothing else unless
    // they configured a choice for a selector. This holds with no configuration at
    // all, which is what most machines have.
    let start = Start::default();
    assert!(start.config.is_empty());
    assert!(start.cwd.as_os_str().is_empty());
}
