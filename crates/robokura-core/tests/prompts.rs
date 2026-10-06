//! What the agent is sent.
//!
//! A prompt is the one place the product's words go straight to a model, so what
//! it says and what it says once is worth a check that needs no agent.

use robokura_core::domain::{Assistant, Kind, Message};
use robokura_core::prompt;

fn assistant() -> Assistant {
    Assistant::new(
        "Letters",
        "drafts my letters",
        "You write in my voice, briefly.",
        "some-agent",
    )
}

fn say(thread_id: &str, seq: i64, body: &str) -> Message {
    Message::person(thread_id, seq, body)
}

#[test]
fn the_assistant_is_told_what_it_is_for_from_the_persons_own_words() {
    let assistant = assistant();
    let text = prompt::purpose(&assistant);

    assert!(text.contains("Letters"), "it is told its name: {text}");
    assert!(text.contains("drafts my letters"), "and its title: {text}");
    assert!(
        text.contains("You write in my voice, briefly."),
        "and its description: {text}"
    );
}

#[test]
fn a_purpose_with_nothing_in_it_still_names_the_assistant() {
    // A blank title or description must not leave a sentence dangling.
    let assistant = Assistant::new("Letters", "  ", "", "");
    let text = prompt::purpose(&assistant);
    assert_eq!(text.trim(), "You are Letters.");
}

#[test]
fn the_question_is_in_the_opening_prompt_exactly_once() {
    // The question is stored before it is sent, so a prompt built from the
    // stored thread would carry it twice and the agent would answer a message
    // that appears to have been asked twice.
    let thread = "t1";
    let history = vec![say(thread, 1, "earlier thing")];
    let text = prompt::opening(&assistant(), &history, "write me a letter");

    assert_eq!(
        text.matches("write me a letter").count(),
        1,
        "the question is there once: {text}"
    );
    assert!(
        text.contains("earlier thing"),
        "and what came before it is there: {text}"
    );
    assert!(
        text.contains("Letters"),
        "and so is what the assistant is for: {text}"
    );
}

#[test]
fn a_reply_that_was_cut_off_is_left_out_of_the_history() {
    // A reply that never finished is shown in the thread as one that was cut
    // off. Repeating it to the agent as something it said is not what happened.
    let thread = "t1";
    let mut cut_off = Message::from_assistant(
        thread,
        2,
        &assistant(),
        Kind::Text,
        "I was going to say",
        false,
    );
    cut_off.complete = false;
    let history = vec![say(thread, 1, "hello"), cut_off];
    let text = prompt::opening(&assistant(), &history, "carry on");

    assert!(
        !text.contains("I was going to say"),
        "it is left out: {text}"
    );
    assert!(
        text.contains("hello"),
        "and what did happen is kept: {text}"
    );
}

#[test]
fn a_later_message_is_only_the_message() {
    // The agent is still running and already holds the rest, so repeating the
    // thread on every turn would cost more each time for nothing.
    assert_eq!(prompt::follow_up("  carry on  "), "carry on");
}
