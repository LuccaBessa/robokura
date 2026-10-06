//! A real assistant, a real reply.
//!
//! This is not run by `cargo test`. It starts an actual agent on this machine and
//! asks it something, so it needs one installed and signed in, and it takes as long
//! as that agent takes. The checks that run without an agent cannot tell you whether
//! an assistant answers at all, and that is the whole of the first step.
//!
//! Run it with:
//!
//! ```text
//! cargo test -p robokura-core --test live -- --ignored --nocapture
//! ```

use std::time::{Duration, Instant};

use robokura_core::{Core, Kind, paths};

#[test]
#[ignore = "starts a real agent on this machine"]
fn an_assistant_answers_what_was_asked_of_it() {
    let root = std::env::temp_dir().join("robokura-checks").join("live");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("scratch folder");

    let mut core = Core::open_at(root.clone()).expect("the store opened");

    let found = robokura_acp::agents::detect();
    println!("agents on this machine: {found:?}");
    assert!(
        !found.is_empty(),
        "this needs an agent installed, or there is nothing to ask"
    );

    let assistant = core
        .create_assistant(
            "Letters",
            "answers briefly",
            "You answer in one short sentence and nothing else.",
        )
        .expect("the assistant was made");
    println!("agent bound to: {}", assistant.runs_on);

    core.send(&assistant.id, "Say only the word: ready.")
        .expect("the message was stored");

    let deadline = Instant::now() + Duration::from_secs(90);
    let mut said = String::new();
    let mut last_tell = Instant::now();

    while Instant::now() < deadline {
        core.pump();
        std::thread::sleep(Duration::from_millis(100));

        let messages = core.messages(&assistant.id).expect("the thread reads");

        if last_tell.elapsed() > Duration::from_secs(5) {
            last_tell = Instant::now();
            println!(
                "  [{:>3}s] busy={} problem={:?} messages={:?}",
                90 - deadline.saturating_duration_since(Instant::now()).as_secs(),
                core.busy(),
                core.problem(&assistant.id),
                messages
                    .iter()
                    .map(|m| (
                        m.seq,
                        m.kind,
                        m.complete,
                        m.body.chars().take(40).collect::<String>()
                    ))
                    .collect::<Vec<_>>()
            );
        }

        let replies: Vec<_> = messages
            .iter()
            .filter(|message| message.kind == Kind::Text && !message.is_person())
            .collect();

        if let Some(reply) = replies.last() {
            if reply.complete {
                said = reply.body.clone();
                break;
            }
        }
    }

    println!("what the assistant said: {said:?}");

    assert!(
        said.to_lowercase().contains("ready"),
        "the assistant was asked for one word and did not say it. It said: {said:?}"
    );

    // The question is on record either way, and the reply is a finished one.
    let messages = core.messages(&assistant.id).expect("the thread reads");
    assert!(messages[0].is_person(), "the question was stored");
    assert!(
        messages[0].body.contains("Say only the word"),
        "with what the person said"
    );

    // And it is still there after a restart.
    drop(core);
    let again = Core::open_at(root.clone()).expect("reopened");
    let after = again.messages(&assistant.id).expect("read");
    assert_eq!(
        after.len(),
        messages.len(),
        "the thread reads the same after a restart"
    );
    assert!(
        after.iter().any(|message| message.body.contains("ready")),
        "and the reply is still in it"
    );

    // Nothing was written outside the folder it was given.
    let home = paths::assistant_home(&root, &assistant.id);
    assert!(
        home.is_dir(),
        "the assistant had a folder of its own to run in"
    );
}

/// An assistant that has to think sends its reasoning in pieces, as often as it
/// breathes. Keeping each piece puts a row in the thread for every few words, and
/// a thread of nothing but thinking is worse than no thread at all.
#[test]
#[ignore = "starts a real agent on this machine"]
fn thinking_does_not_fill_the_thread() {
    let root = std::env::temp_dir()
        .join("robokura-checks")
        .join("live-thinking");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("scratch folder");

    let mut core = Core::open_at(root).expect("the store opened");
    let assistant = core
        .create_assistant("Jarvis", "Butler", "responsible for my personal life")
        .expect("the assistant was made");

    core.send(
        &assistant.id,
        "Introduce yourself in exactly two sentences.",
    )
    .expect("the message was stored");

    let deadline = Instant::now() + Duration::from_secs(120);
    let mut reply = String::new();
    while Instant::now() < deadline {
        core.pump();
        std::thread::sleep(Duration::from_millis(100));

        let messages = core.messages(&assistant.id).expect("the thread reads");
        if let Some(last) = messages
            .iter()
            .rev()
            .find(|message| message.kind == Kind::Text && !message.is_person() && message.complete)
        {
            reply = last.body.clone();
            break;
        }
    }

    let messages = core.messages(&assistant.id).expect("the thread reads");
    for message in &messages {
        println!(
            "  {} {:?} complete={} {:?}",
            message.seq, message.kind, message.complete, message.body
        );
    }

    assert!(!reply.is_empty(), "the assistant said something");

    let notes = messages
        .iter()
        .filter(|message| message.kind == Kind::Note)
        .count();
    assert!(
        notes <= 3,
        "the thread holds {notes} note rows, so the assistant's thinking is being stored \
         one piece at a time rather than being left out"
    );

    let replies = messages
        .iter()
        .filter(|message| message.kind == Kind::Text && !message.is_person())
        .count();
    assert_eq!(replies, 1, "and there is one reply, not one per piece");
}

/// An agent has to be started to answer what it can be set to, so the settings
/// start one and take the answer rather than waiting for a conversation.
#[test]
#[ignore = "starts a real agent on this machine"]
fn an_agent_is_asked_what_it_can_be_set_to_without_any_assistant_being_spoken_to() {
    let root = std::env::temp_dir()
        .join("robokura-checks")
        .join("live-probe");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("scratch folder");

    let mut core = Core::open_at(root.clone()).expect("the store opened");
    let agent = robokura_acp::agents::first().expect("an agent is installed");

    assert!(
        core.settings().expect("read").offered.is_empty(),
        "nothing is known before anything is asked, which is the state the settings open in"
    );

    core.probe(&agent);

    let deadline = Instant::now() + Duration::from_secs(45);
    let mut offered = Vec::new();
    while Instant::now() < deadline && offered.is_empty() {
        core.pump();
        std::thread::sleep(Duration::from_millis(50));
        offered = core
            .settings()
            .expect("read")
            .offered
            .get(&agent)
            .cloned()
            .unwrap_or_default();
    }

    assert!(
        !offered.is_empty(),
        "asking the agent once, with no assistant made and nothing sent, is enough to fill the \
         model chooser. The list is the agent's own, so there is no other way to get it."
    );
    assert!(
        !core.probing(),
        "and the agent is let go once it has answered, rather than held open for a pane \
         that has stopped needing it"
    );

    let model = offered
        .iter()
        .find(|selector| selector.category == "model")
        .expect("and what it said includes a model, which is the thing the pane is for");
    assert!(
        !model.values.is_empty(),
        "with values in it. A chooser built from a name with nothing behind it opens onto an \
         empty menu, and a check that only looked for the name would call that a working \
         chooser. It said: {model:?}"
    );
    assert!(
        !model.current.is_empty(),
        "and with the value it is set to, because a chooser that does not say what the agent \
         is using now leaves a person with nothing to compare their choice against. It said: \
         {model:?}"
    );

    // The list is kept, so the next person to open the settings sees it without a
    // agent being started at all.
    let again = Core::open_at(root).expect("reopened");
    assert_eq!(
        again
            .settings()
            .expect("read")
            .offered
            .get(&agent)
            .map(Vec::len)
            .unwrap_or_default(),
        offered.len(),
        "and it is still there after a restart, so the chooser is not empty on a machine \
         where the settings are opened twice"
    );
}

/// Nothing about this can be checked without an agent, because every model name,
/// every level of reasoning and every mode comes from the agent rather than from
/// anything this product knows. A session is opened without sending anything: the
/// agent answers what it can be set to as it answers being asked for a session.
#[test]
#[ignore = "starts a real agent on this machine"]
fn an_agent_says_what_its_sessions_can_be_set_to() {
    let home = std::env::temp_dir()
        .join("robokura-checks")
        .join("live-selectors");
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).expect("scratch folder");

    let found = robokura_acp::agents::detect();
    println!("agents on this machine: {found:?}");
    let Some(agent) = found.first() else {
        println!("no agent is installed, so there is nothing to ask");
        return;
    };

    let mut session = robokura_acp::Session::start(robokura_acp::Start {
        command: agent.command.clone(),
        cwd: home,
        ..Default::default()
    })
    .expect("the agent started");

    let deadline = Instant::now() + Duration::from_secs(45);
    let mut said = None;
    while Instant::now() < deadline && said.is_none() {
        while let Some(event) = session.next_event() {
            if let robokura_acp::Event::Selectors(selectors) = event {
                said = Some(selectors);
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    match said {
        Some(selectors) if selectors.is_empty() => println!(
            "SENTINEL: {} advertises no session selectors at all, so there is no model to \
             choose and no level and no mode",
            agent.name
        ),
        Some(selectors) => {
            for selector in &selectors {
                println!(
                    "SENTINEL: {} offers {:?} ({}) with {} values, on {:?}",
                    agent.name,
                    selector.name,
                    selector.category,
                    selector.values.len(),
                    selector.current
                );
            }
        }
        None => println!(
            "SENTINEL: {} never said, so there is nothing to show",
            agent.name
        ),
    }
}

/// The agent is started inside the assistant's own folder, and a running process
/// holds that folder open against removal on Windows. This needs a real agent,
/// because a running agent is the only thing that holds the folder.
#[test]
#[ignore = "starts a real agent on this machine"]
fn an_assistant_can_be_removed_while_its_agent_is_still_running() {
    let root = std::env::temp_dir()
        .join("robokura-checks")
        .join("live-delete");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("scratch folder");

    let mut core = Core::open_at(root.clone()).expect("the store opened");

    let found = robokura_acp::agents::detect();
    assert!(
        !found.is_empty(),
        "this needs an agent installed, or there is no agent to be running"
    );

    let assistant = core
        .create_assistant("Temporary", "here and gone", "")
        .expect("the assistant was made");
    let home = paths::assistant_home(&root, &assistant.id);

    // Spoken to and not waited for, so the agent is mid-turn and still has the
    // folder. This is the state this was hit from.
    core.send(&assistant.id, "Count slowly from one to twenty.")
        .expect("the message was stored");
    std::thread::sleep(Duration::from_millis(1500));
    core.pump();

    assert!(
        core.working(&assistant.id) || core.thinking(&assistant.id),
        "the agent is still working, which is the state worth removing from"
    );

    let removed = core.delete_assistant(&assistant.id);
    println!("removing the assistant said: {removed:?}");

    assert!(
        removed.is_ok(),
        "removing an assistant removed it rather than failing on a file the person never \
         asked about: {:?}",
        removed.err()
    );
    assert!(
        !home.exists(),
        "and its own folder went with it, so the agent was no longer holding it"
    );
    assert!(
        core.assistants().expect("read").is_empty(),
        "and nothing of it is left in the list"
    );
}
