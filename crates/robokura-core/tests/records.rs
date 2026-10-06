//! What is kept and where it is. Runs without a window.
//!
//! Every check here opens a store under a scratch folder it makes itself. A
//! check that reached the real data directory would leave files in a person's
//! own assistants, so the root is always a parameter and never found from the
//! environment.

use std::path::{Path, PathBuf};

use robokura_core::{Assistant, Core, Kind, Message, Settings, paths, store::Store};

fn scratch(name: &str) -> PathBuf {
    let base = std::env::temp_dir().join("robokura-checks").join(name);
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).expect("scratch folder");
    base
}

fn core(name: &str) -> Core {
    Core::open_at(scratch(name)).expect("the store opened")
}

/// Writes a message into an assistant's thread without asking anything to answer it.
///
/// `send` starts an agent wherever one is installed on the machine, so what a thread
/// ends with would depend on the machine rather than on the check.
fn say(root: &Path, assistant: &Assistant, build: impl FnOnce(&str, i64) -> Message) {
    let store = Store::open(&paths::db_file(root)).expect("the store opened");
    let thread = store
        .thread_for(&assistant.id)
        .expect("read")
        .expect("the assistant has a thread");
    let seq = store.next_seq(&thread.id).expect("read");
    store.add_message(&build(&thread.id, seq)).expect("stored");
}

/// The same, said at a moment of the check's own choosing. Times are in seconds and
/// everything written in one check lands in the same second, so an order between two
/// threads has to be stamped rather than waited for.
fn said_at(root: &Path, assistant: &Assistant, at: i64, body: &str) {
    say(root, assistant, |thread, seq| {
        let mut message = Message::person(thread, seq, body);
        message.created_at = at;
        message.updated_at = at;
        message
    });
}

#[test]
fn an_assistant_and_its_thread_survive_being_closed_and_reopened() {
    let root = scratch("survives");

    let id = {
        let mut core = Core::open_at(root.clone()).expect("the store opened");
        let assistant = core
            .create_assistant("Letters", "drafts my letters", "writes in my voice")
            .expect("the assistant was made");
        assistant.id
    };

    // Reopened from nothing but the folder.
    let again = Core::open_at(root).expect("the store reopened");
    let assistants = again.assistants().expect("the assistants were read");

    assert_eq!(assistants.len(), 1, "the assistant is still there");
    assert_eq!(assistants[0].name, "Letters", "with its name");
    assert_eq!(assistants[0].title, "drafts my letters", "and its title");
    assert_eq!(
        assistants[0].description, "writes in my voice",
        "and what it is for"
    );
    assert_eq!(id, assistants[0].id, "under the same identifier");
}

#[test]
fn an_assistant_has_exactly_one_thread_and_the_database_says_so() {
    let mut core = core("one-thread");
    let first = core
        .create_assistant("Letters", "drafts", "")
        .expect("the assistant was made");
    let second = core
        .create_assistant("Research", "finds sources", "")
        .expect("the second assistant was made");

    let one = core.messages(&first.id).expect("the first thread reads");
    let two = core.messages(&second.id).expect("the second thread reads");
    assert!(one.is_empty(), "a new assistant has an empty thread");
    assert!(two.is_empty(), "and so does the next one");

    // Both assistants exist and neither took the other's thread.
    assert_eq!(core.assistants().expect("read").len(), 2);
}

#[test]
fn what_the_person_said_is_stored_in_the_order_it_was_said() {
    let mut core = core("order");
    let assistant = core
        .create_assistant("Letters", "drafts", "")
        .expect("made");

    for text in ["first", "second", "third"] {
        core.send(&assistant.id, text)
            .expect("the message was stored");
    }

    let messages = core.messages(&assistant.id).expect("the thread reads");
    let said: Vec<&str> = messages
        .iter()
        .filter(|message| message.is_person())
        .map(|message| message.body.as_str())
        .collect();

    assert_eq!(
        said,
        ["first", "second", "third"],
        "in the order it was said"
    );

    let positions: Vec<i64> = messages.iter().map(|message| message.seq).collect();
    let mut rising = positions.clone();
    rising.sort_unstable();
    rising.dedup();
    assert_eq!(positions, rising, "positions only go up and never repeat");
}

#[test]
fn a_message_is_written_before_the_assistant_is_asked() {
    let mut core = core("stored-first");
    let assistant = core
        .create_assistant("Letters", "drafts", "")
        .expect("made");
    core.send(&assistant.id, "write me a letter")
        .expect("the send did not fail");

    let messages = core.messages(&assistant.id).expect("the thread reads");

    // The question comes first whether or not an agent was found on this machine, and
    // whether or not it managed to answer.
    assert!(!messages.is_empty(), "something was stored");
    assert_eq!(
        messages[0].body, "write me a letter",
        "the question was stored"
    );
    assert!(messages[0].is_person(), "and it is the person's message");
    assert!(messages[0].complete, "and it is a finished one");

    for later in &messages[1..] {
        assert!(
            later.seq > messages[0].seq,
            "anything after it comes later in the thread"
        );
    }
}

/// The rows themselves, counted rather than reached through the product.
///
/// Reading an assistant's thread after deleting the assistant returns nothing whether
/// the thread was deleted or merely cannot be found any more, and deleting an
/// assistant is irreversible, so this is checked against the database.
#[test]
fn deleting_an_assistant_removes_its_thread_and_its_messages_from_the_database() {
    let root = scratch("deletes-rows");
    let id = {
        let mut core = Core::open_at(root.clone()).expect("opened");
        let assistant = core
            .create_assistant("Temporary", "here and gone", "")
            .expect("made");
        core.send(&assistant.id, "hello").expect("stored");
        core.send(&assistant.id, "and again").expect("stored");

        let count = |table: &str| -> i64 {
            rusqlite::Connection::open(paths::db_file(&root))
                .expect("the database opened")
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .expect("counted")
        };
        assert_eq!(count("thread"), 1, "the assistant has a thread to lose");
        assert!(count("message") >= 2, "the thread has messages to lose");
        assistant.id
    };

    let mut core = Core::open_at(root.clone()).expect("reopened");
    core.delete_assistant(&id)
        .expect("the assistant was removed");

    let db = rusqlite::Connection::open(paths::db_file(&root)).expect("the database opened");
    let remaining = |table: &str, column: &str, value: &str| -> i64 {
        db.query_row(
            &format!("SELECT COUNT(*) FROM {table} WHERE {column} = ?1"),
            [value],
            |row| row.get(0),
        )
        .expect("counted")
    };

    assert_eq!(
        remaining("assistant", "id", &id),
        0,
        "the assistant row is gone"
    );
    assert_eq!(
        remaining("thread", "assistant_id", &id),
        0,
        "its thread is gone, not just unreachable"
    );
    assert_eq!(
        remaining("message", "thread_id", &id),
        0,
        "and so are the messages that were in it"
    );

    // Nothing is left anywhere in the file, not even for another assistant to find.
    let orphans: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM message WHERE thread_id NOT IN (SELECT id FROM thread)",
            [],
            |row| row.get(0),
        )
        .expect("counted");
    assert_eq!(
        orphans, 0,
        "no message is left pointing at a thread that is not there"
    );

    let threads_without_an_assistant: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM thread WHERE assistant_id NOT IN (SELECT id FROM assistant)",
            [],
            |row| row.get(0),
        )
        .expect("counted");
    assert_eq!(
        threads_without_an_assistant, 0,
        "no thread is left pointing at an assistant that is not there"
    );

    let _ = id;
}

#[test]
fn a_new_assistant_starts_with_words_and_nothing_else_asked() {
    let root = scratch("new-assistant-words");
    let id = {
        let mut core = Core::open_at(root.clone()).expect("the store opened");
        let assistant = core
            .create_default_assistant()
            .expect("the assistant was made");
        assistant.id
    };

    let core = Core::open_at(root.clone()).expect("the store reopened");
    let assistants = core.assistants().expect("the assistants were read");

    assert_eq!(assistants.len(), 1, "one control made one assistant");
    assert_eq!(
        assistants[0].name,
        robokura_core::NEW_ASSISTANT_NAME,
        "it is called New assistant until someone names it"
    );
    assert_eq!(
        assistants[0].title,
        robokura_core::NEW_ASSISTANT_TITLE,
        "and it has no title. A title says what the assistant is for, and a placeholder \
         would be a claim about it that nobody made."
    );
    assert_eq!(
        assistants[0].description,
        robokura_core::NEW_ASSISTANT_DESCRIPTION,
        "and it says what it is until someone says otherwise"
    );

    // The words are stored rather than held in a field, so they survive a
    // restart and they are what the agent is sent.
    assert!(
        core.messages(&id).expect("the thread reads").is_empty(),
        "a new assistant has a thread, and it starts empty"
    );
    assert_eq!(
        robokura_core::prompt::purpose(&assistants[0]),
        format!(
            "You are {}. {}",
            robokura_core::NEW_ASSISTANT_NAME,
            robokura_core::NEW_ASSISTANT_DESCRIPTION
        ),
        "what the person has not written yet is what the agent is told. It is not a \
         placeholder that stops being sent."
    );
}

#[test]
fn each_press_of_the_one_control_makes_its_own_assistant() {
    let mut core = core("one-control");
    let first = core.create_default_assistant().expect("made");
    let second = core.create_default_assistant().expect("made again");

    assert_ne!(first.id, second.id, "two presses make two assistants");
    assert_eq!(
        core.assistants().expect("read").len(),
        2,
        "and neither replaced the other"
    );

    // Two assistants sharing one name is not the same as one assistant, because the
    // person picks between them by thread and by pane.
    assert!(
        core.messages(&first.id).expect("read").is_empty()
            && core.messages(&second.id).expect("read").is_empty(),
        "each has its own thread"
    );
}

#[test]
fn an_agent_still_holding_its_folder_is_said_in_words_rather_than_as_a_file_error() {
    // What someone saw was "a file could not be written: The process cannot access the
    // file because it is being used by another process. (os error 32)". They were
    // removing an assistant. They had no file in mind.
    let locked = std::io::Error::from_raw_os_error(32);
    let said = robokura_core::Error::AgentStillRunning(locked).to_string();

    assert!(
        said.contains("agent is still running") && said.contains("still running"),
        "the words name the thing that is actually in the way: {said}"
    );
    assert!(
        said.contains("still here") && said.contains("try again"),
        "and they say what it means for the assistant and what to do about it: {said}"
    );
    assert!(
        !said.contains("os error"),
        "and none of the operating system's complaint reaches the person: {said}"
    );
}

#[test]
fn a_deleted_assistant_leaves_nothing_behind() {
    let root = scratch("deletes");
    let id = {
        let mut core = Core::open_at(root.clone()).expect("opened");
        let assistant = core
            .create_assistant("Temporary", "here and gone", "")
            .expect("made");
        core.send(&assistant.id, "hello").expect("stored");
        assistant.id
    };

    let mut core = Core::open_at(root).expect("reopened");
    core.delete_assistant(&id)
        .expect("the assistant was removed");

    assert!(
        core.assistants().expect("read").is_empty(),
        "the list is empty"
    );
    assert!(
        core.messages(&id).expect("read").is_empty(),
        "its thread is empty"
    );

    let again = Core::open_at(scratch("deletes")).expect("reopened again");
    assert!(
        again.assistants().expect("read").is_empty(),
        "and it stays gone after another restart"
    );
}

#[test]
fn the_name_of_whoever_wrote_a_message_stays_when_the_assistant_is_renamed() {
    // The author's name is copied onto the message, so history does not change
    // because a label did.
    let mut core = core("attribution");
    let assistant = core
        .create_assistant("Letters", "drafts", "")
        .expect("made");
    core.send(&assistant.id, "hello").expect("stored");

    let messages = core.messages(&assistant.id).expect("read");
    assert!(messages[0].is_person(), "the person wrote that one");

    // A note written by an assistant carries the assistant's name at the time.
    let note = robokura_core::Message::from_assistant(
        &core.messages(&assistant.id).expect("read")[0].thread_id,
        99,
        &assistant,
        Kind::Note,
        "working",
        true,
    );
    assert_eq!(note.author_name, "Letters", "the name is on the message");
    assert_eq!(note.assistant_id.as_deref(), Some(assistant.id.as_str()));
}

#[test]
fn a_list_row_carries_the_last_thing_said_in_a_thread_and_when_it_was_said() {
    let root = scratch("previews");
    let mut core = Core::open_at(root.clone()).expect("the store opened");
    let letters = core
        .create_assistant("Letters", "drafts", "")
        .expect("made");
    let research = core
        .create_assistant("Research", "finds sources", "")
        .expect("made");

    say(&root, &letters, |thread, seq| {
        Message::person(thread, seq, "the first thing I asked")
    });
    say(&root, &letters, |thread, seq| {
        Message::person(thread, seq, "the thing I asked last")
    });

    let previews = core.previews().expect("read");

    assert!(
        !previews.iter().any(|p| p.assistant_id == research.id),
        "an assistant with nothing said in its thread is left out rather than turned into an \
         empty one, because there is no second line to draw for it"
    );
    assert_eq!(
        previews.len(),
        1,
        "so one row of the two has something to say under its name and the other has not"
    );
    assert_eq!(
        previews[0].assistant_id, letters.id,
        "and the one that is there is that assistant's"
    );
    assert_eq!(
        previews[0].body, "the thing I asked last",
        "the newest message, not the first and not the oldest. A row is read to find out where \
         a conversation got to."
    );

    // Read back from the thread rather than taken from the row, so the two cannot be two
    // different claims.
    let thread = core.messages(&letters.id).expect("read");
    assert_eq!(
        previews[0].at,
        thread.last().expect("the last message").created_at,
        "and it is when that message was said"
    );
}

#[test]
fn a_reply_that_has_not_been_written_yet_does_not_blank_a_row() {
    // The conversation leaves an unwritten reply out rather than drawing an empty
    // bubble, and the list has to leave it out too.
    let root = scratch("previews-mid-answer");
    let mut core = Core::open_at(root.clone()).expect("the store opened");
    let assistant = core
        .create_assistant("Letters", "drafts", "")
        .expect("made");

    say(&root, &assistant, |thread, seq| {
        Message::person(thread, seq, "write me a letter")
    });
    say(&root, &assistant, |thread, seq| {
        Message::from_assistant(thread, seq, &assistant, Kind::Text, "", false)
    });

    let previews = core.previews().expect("read");

    assert_eq!(
        previews.len(),
        1,
        "the thread still says something while the assistant is working on it"
    );
    assert_eq!(
        previews[0].body, "write me a letter",
        "and what it says is the last thing that was actually said. A row that went blank every \
         time an assistant started a reply would be blank for most of the time an assistant \
         spends working, and blank reads as empty rather than as busy."
    );
}

#[test]
fn a_row_carries_a_cut_of_a_very_long_reply_rather_than_all_of_it() {
    let root = scratch("previews-cut");
    let mut core = Core::open_at(root.clone()).expect("the store opened");
    let assistant = core
        .create_assistant("Letters", "drafts", "")
        .expect("made");

    let long = "a long reply. ".repeat(400);
    say(&root, &assistant, |thread, seq| {
        Message::from_assistant(thread, seq, &assistant, Kind::Text, long.clone(), true)
    });

    let preview = core.previews().expect("read").remove(0);
    assert!(
        preview.body.chars().count() < long.chars().count(),
        "the row does not carry a whole long reply. The list is drawn on every redraw and reads \
         this for every assistant it draws, so the whole of every reply would be carried and \
         then ellipsised away a few dozen characters later."
    );
    assert!(
        long.starts_with(preview.body.as_str()),
        "and what it carries is the start of it, which is the part a row has room for"
    );
}

/// The names of every assistant, in the order the store lists them.
fn names_in_order(core: &Core) -> Vec<String> {
    core.assistants()
        .expect("the assistants read")
        .into_iter()
        .map(|assistant| assistant.name)
        .collect()
}

#[test]
fn the_list_leads_with_the_thread_that_was_spoken_in_last() {
    let root = scratch("list-order");
    let mut core = Core::open_at(root.clone()).expect("the store opened");

    let letters = core
        .create_assistant("Letters", "drafts", "")
        .expect("made");
    let research = core
        .create_assistant("Research", "finds sources", "")
        .expect("made");

    // Both stamped forward from when they were made, so the order comes from the
    // conversation rather than from the order the store happens to read them back in.
    said_at(&root, &letters, 100, "asked a while ago");
    said_at(&root, &research, 300, "asked just now");

    let listed = names_in_order(&core);

    assert_eq!(
        listed,
        ["Research", "Letters"],
        "the thread spoken in last is at the top of the list. An order taken from when each \
         assistant was made would put Letters first, and somebody coming back to the window is \
         coming back to the conversation they were last having."
    );
}

#[test]
fn an_assistant_nobody_has_spoken_to_is_placed_by_when_it_was_made() {
    let root = scratch("list-order-unused");
    let mut core = Core::open_at(root.clone()).expect("the store opened");

    let older = core
        .create_assistant("Letters", "drafts", "")
        .expect("made");
    core.create_assistant("Research", "finds sources", "")
        .expect("made");

    // Said before Research existed, so the only thing that can put Letters at the top
    // is the moment its thread was last spoken in.
    said_at(&root, &older, 1, "asked long ago and never again");

    let listed = names_in_order(&core);

    assert_eq!(
        listed,
        ["Research", "Letters"],
        "an assistant with nothing said in its thread stands where it was made, which is above \
         one that was spoken in before it existed. A new assistant at the top of the list is \
         what somebody expects of one they have just made."
    );
}

#[test]
fn a_reply_still_arriving_does_not_move_a_thread_up_the_list() {
    // The reply row is stored before the agent has said a word, so ordering by the
    // newest row of any kind would put a thread at the top on the strength of a
    // message nobody can read yet.
    let root = scratch("list-order-mid-answer");
    let mut core = Core::open_at(root.clone()).expect("the store opened");
    let letters = core
        .create_assistant("Letters", "drafts", "")
        .expect("made");
    let research = core
        .create_assistant("Research", "finds sources", "")
        .expect("made");

    said_at(&root, &research, 300, "asked just now");
    said_at(&root, &letters, 100, "asked a while ago");
    say(&root, &letters, |thread, seq| {
        Message::from_assistant(thread, seq, &letters, Kind::Text, "", false)
    });

    let listed = names_in_order(&core);

    assert_eq!(
        listed,
        ["Research", "Letters"],
        "the thread whose reply is still arriving keeps the place its last said message earned \
         it. The row shows that message and when it was said, so a thread that jumped to the \
         top of the list on a message the row does not show would be two claims that disagree."
    );
}

#[test]
fn what_the_person_configured_survives_being_closed_and_reopened() {
    let root = scratch("settings-survive");
    {
        let mut core = Core::open_at(root.clone()).expect("the store opened");
        let mut settings = core.settings().expect("the settings read");

        settings.runs_on = r#""C:\elsewhere\opencode.exe" acp"#.to_string();
        settings
            .chosen
            .insert("model".to_string(), "some-model".to_string());
        settings.offered.insert(
            r#""C:\elsewhere\opencode.exe" acp"#.to_string(),
            vec![robokura_core::ConfigOption {
                id: "model".to_string(),
                name: "Model".to_string(),
                category: "model".to_string(),
                values: vec![("some-model".to_string(), "One".to_string())],
                current: "some-model".to_string(),
            }],
        );

        core.save_settings(&settings)
            .expect("the settings were written");
    }

    let core = Core::open_at(root).expect("the store reopened");
    let read = core.settings().expect("the settings read");

    assert_eq!(
        read,
        core.settings().expect("read again"),
        "and reading them twice says the same thing, so a settings pane drawn from them \
         cannot show one thing and save another"
    );
    assert_eq!(read.runs_on, r#""C:\elsewhere\opencode.exe" acp"#);
    assert_eq!(
        read.chosen.get("model").map(String::as_str),
        Some("some-model")
    );
    assert_eq!(
        read.offered.len(),
        1,
        "and what an agent offered is kept too"
    );
}

/// The mode is kept with the rest of what this machine has been told, so it survives the
/// window being closed the same way the agent does.
#[test]
fn the_mode_a_person_chose_is_still_there_after_a_restart() {
    let root = scratch("mode-survive");
    {
        let mut core = Core::open_at(root.clone()).expect("the store opened");
        let mut settings = core.settings().expect("the settings read");
        settings.mode = "dark".to_string();
        core.save_settings(&settings)
            .expect("the settings were written");
    }

    let core = Core::open_at(root).expect("the store reopened");
    assert_eq!(
        core.settings().expect("read").mode,
        "dark",
        "a person who chose dark mode gets it back on the next run rather than the machine's \
         answer again"
    );
}

#[test]
fn a_machine_with_nothing_configured_gets_the_settings_that_ship() {
    let core = core("settings-defaults");
    let settings = core.settings().expect("the settings read");

    assert_eq!(
        settings,
        Settings::default(),
        "a missing setting is not an error and is not a row of nulls. It is a setting nobody \
         has changed yet."
    );
    assert!(
        settings.runs_on.is_empty(),
        "and no agent is chosen, which means the first one found on this machine"
    );
    assert!(
        settings.mode.is_empty(),
        "and no mode is chosen, which means the machine's own appearance. An empty mode is an \
         answer rather than a missing setting."
    );
}

#[test]
fn what_an_agent_offered_comes_back_whole() {
    // The names, the values and the value in force are the whole of what the model
    // chooser is made of. All three are written as the record's own shape and have to
    // come back as it, because a chooser drawn from a name with no values behind it
    // opens onto nothing at all.
    let root = scratch("selectors-round-trip");
    let offered = vec![robokura_core::ConfigOption {
        id: "model".to_string(),
        name: "Model".to_string(),
        category: "model".to_string(),
        values: vec![
            ("one/small".to_string(), "Small".to_string()),
            ("one/large".to_string(), "Large".to_string()),
        ],
        current: "one/large".to_string(),
    }];

    {
        let mut core = Core::open_at(root.clone()).expect("the store opened");
        let mut settings = core.settings().expect("read");
        settings
            .offered
            .insert("some agent".to_string(), offered.clone());
        core.save_settings(&settings).expect("written");
    }

    let core = Core::open_at(root).expect("the store reopened");
    let back = core
        .settings()
        .expect("read")
        .offered
        .get("some agent")
        .cloned()
        .expect("what was written is there");

    assert_eq!(
        back, offered,
        "the values and the value in force both survive a write and a read. Reading these back \
         by picking fields out of the text by hand is what lost them once: the names came back \
         and the values did not, so the rows were there and every chooser was empty."
    );
}

#[test]
fn a_selector_that_cannot_be_read_is_left_out_rather_than_half_read() {
    let root = scratch("selectors-damaged");
    {
        let mut core = Core::open_at(root.clone()).expect("the store opened");
        core.save_settings(&Settings::default())
            .expect("the settings were written");
        rusqlite::Connection::open(paths::db_file(&root))
            .expect("the database opened")
            .execute(
                "UPDATE setting SET value = '[{\"id\":\"model\",\"values\":\"not a list\"}]' \
                 WHERE key = 'offered'",
                [],
            )
            .expect("the row was damaged");
    }

    let core = Core::open_at(root).expect("the store reopened");
    let offered = core.settings().expect("read").offered;

    assert!(
        offered.values().all(Vec::is_empty),
        "a selector nobody can read is left out rather than half read, because half of one is \
         a chooser with a name on it that opens onto nothing. Got {offered:?}"
    );
}

#[test]
fn a_setting_written_by_something_else_is_left_out_rather_than_refusing_to_open() {
    let root = scratch("settings-damaged");
    {
        let mut core = Core::open_at(root.clone()).expect("the store opened");
        core.save_settings(&Settings::default())
            .expect("the settings were written");
        rusqlite::Connection::open(paths::db_file(&root))
            .expect("the database opened")
            .execute(
                "UPDATE setting SET value = 'not json at all' WHERE key = 'chosen'",
                [],
            )
            .expect("the row was damaged");
    }

    let core = Core::open_at(root).expect("the store reopened");
    let settings = core.settings().expect("the settings read");

    assert!(
        settings.chosen.is_empty(),
        "a choice nobody can read comes back empty rather than as an error. Refusing to open \
         the window over one damaged row would make a person lose the agent over it."
    );
    assert!(
        settings.runs_on.is_empty(),
        "and the rest of it still reads, because one bad row is not a reason to lose the rest"
    );
}

/// A setting this version does not know is left where it is.
///
/// An older version of the application opening a file written by a newer one must
/// not quietly strip a setting it does not recognise, or a person who goes back to
/// an older build loses what they chose.
#[test]
fn a_setting_this_version_does_not_know_is_left_where_it_is() {
    let root = scratch("settings-future");
    {
        let mut core = Core::open_at(root.clone()).expect("the store opened");
        core.save_settings(&Settings::default())
            .expect("the settings were written");
        rusqlite::Connection::open(paths::db_file(&root))
            .expect("the database opened")
            .execute(
                "INSERT INTO setting (key, value) VALUES ('from_the_future', 'something')",
                [],
            )
            .expect("the row was written");
    }

    let mut core = Core::open_at(root.clone()).expect("the store reopened");
    let mut settings = core.settings().expect("the settings read");
    settings.runs_on = "changed".to_string();
    core.save_settings(&settings)
        .expect("the settings were written");

    let left = rusqlite::Connection::open(paths::db_file(&root))
        .expect("the database opened")
        .query_row(
            "SELECT value FROM setting WHERE key = 'from_the_future'",
            [],
            |row| row.get::<_, String>(0),
        );
    assert_eq!(
        left.as_deref(),
        Ok("something"),
        "writing the settings again left a key it does not know alone. An older build opening \
         a newer file must not strip a setting a person chose."
    );
}

/// The migration only ever runs against a file an earlier build wrote. A database this
/// build creates already has the new names, so the migration's own steps are no-ops on
/// it. The old shape is written out rather than imported, because the point is that it
/// is what a person already has on their machine.
const BEFORE_THE_RENAME: &str = "
CREATE TABLE agent (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    title       TEXT NOT NULL,
    description TEXT NOT NULL,
    program     TEXT NOT NULL,
    created_at  INTEGER NOT NULL
);

CREATE TABLE thread (
    id         TEXT PRIMARY KEY,
    agent_id   TEXT NOT NULL UNIQUE REFERENCES agent(id) ON DELETE CASCADE,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE message (
    id          TEXT PRIMARY KEY,
    thread_id   TEXT NOT NULL REFERENCES thread(id) ON DELETE CASCADE,
    seq         INTEGER NOT NULL,
    kind        TEXT NOT NULL,
    agent_id    TEXT REFERENCES agent(id) ON DELETE SET NULL,
    author_name TEXT NOT NULL,
    body        TEXT NOT NULL,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL,
    complete    INTEGER NOT NULL,
    UNIQUE (thread_id, seq)
);

CREATE TABLE setting (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

PRAGMA user_version = 2;
";

#[test]
fn a_database_written_before_the_rename_is_opened_without_losing_anything() {
    let root = scratch("rename-migration");
    std::fs::create_dir_all(&root).expect("scratch folder");

    {
        let db = rusqlite::Connection::open(paths::db_file(&root)).expect("the database opened");
        db.execute_batch(BEFORE_THE_RENAME)
            .expect("the old shape was written");
        db.execute(
            "INSERT INTO agent (id, name, title, description, program, created_at)
             VALUES ('a1', 'Letters', 'drafts my letters', 'writes in my voice', 'opencode acp', 10)",
            [],
        )
        .expect("the assistant was written");
        db.execute(
            "INSERT INTO thread (id, agent_id, created_at, updated_at)
             VALUES ('t1', 'a1', 10, 10)",
            [],
        )
        .expect("the thread was written");
        db.execute(
            "INSERT INTO message
                 (id, thread_id, seq, kind, agent_id, author_name, body,
                  created_at, updated_at, complete)
             VALUES ('m1', 't1', 1, 'text', NULL, 'You', 'hello', 10, 10, 1)",
            [],
        )
        .expect("the question was written");
        db.execute(
            "INSERT INTO message
                 (id, thread_id, seq, kind, agent_id, author_name, body,
                  created_at, updated_at, complete)
             VALUES ('m2', 't1', 2, 'text', 'a1', 'Letters', 'a letter', 11, 11, 1)",
            [],
        )
        .expect("the reply was written");
        db.execute(
            "INSERT INTO setting (key, value) VALUES ('program', 'opencode acp')",
            [],
        )
        .expect("the chosen agent was written");
    }

    let core = Core::open_at(root.clone()).expect("the migration ran on the old file");

    let assistants = core.assistants().expect("the assistants read");
    assert_eq!(
        assistants.len(),
        1,
        "the assistant is still there after the tables were renamed"
    );
    assert_eq!(assistants[0].name, "Letters");
    assert_eq!(assistants[0].title, "drafts my letters");
    assert_eq!(assistants[0].description, "writes in my voice");
    assert_eq!(
        assistants[0].runs_on, "opencode acp",
        "and the agent it runs on is still attached to it. A rename that dropped it would leave \
         every existing assistant unable to answer."
    );

    let messages = core.messages(&assistants[0].id).expect("the thread reads");
    assert_eq!(
        messages.len(),
        2,
        "both messages are still in the thread. The column linking a message to the assistant \
         was renamed, and getting that wrong leaves a conversation with nobody in it."
    );
    assert!(
        messages[0].is_person(),
        "the question is still the person's"
    );
    assert_eq!(
        messages[1].assistant_id.as_deref(),
        Some(assistants[0].id.as_str()),
        "and the reply is still the assistant's, under the assistant it was written by."
    );

    assert_eq!(
        core.settings().expect("the settings read").runs_on,
        "opencode acp",
        "the agent the person had chosen is still chosen. The setting's key was renamed rather \
         than dropped, so being called something else does not cost a person their choice."
    );
}

#[test]
fn an_assistant_is_bound_to_the_agent_the_person_chose() {
    let root = scratch("settings-agent");
    let mut core = Core::open_at(root.clone()).expect("the store opened");

    let chosen = robokura_acp::agents::detect()
        .first()
        .map(|found| found.command.clone());
    let Some(chosen) = chosen else {
        // Nothing installed, so there is nothing to choose and nothing to check.
        return;
    };
    let other = robokura_acp::agents::detect()
        .into_iter()
        .nth(1)
        .map(|found| found.command);

    let mut settings = core.settings().expect("the settings read");
    settings.runs_on = chosen.clone();
    core.save_settings(&settings)
        .expect("the settings were written");

    let assistant = core
        .create_default_assistant()
        .expect("the assistant was made");
    assert_eq!(
        assistant.runs_on, chosen,
        "the agent the person chose is the one a new assistant is bound to"
    );

    // An assistant already made keeps the agent it was made with. A setting that
    // answered for it would be changing which software is acting underneath it.
    if let Some(other) = other {
        let mut settings = core.settings().expect("the settings read");
        settings.runs_on = other.clone();
        core.save_settings(&settings)
            .expect("the settings were written");

        let again = core
            .assistant(&assistant.id)
            .expect("read")
            .expect("still there");
        assert_eq!(
            again.runs_on, chosen,
            "and changing the setting does not change it for an assistant that already exists"
        );
    }
}

#[test]
fn nothing_stored_holds_a_credential() {
    let root = scratch("no-secret");
    let mut core = Core::open_at(root.clone()).expect("opened");
    let assistant = core
        .create_assistant("Letters", "drafts", "writes in my voice")
        .expect("made");
    core.send(&assistant.id, "hello").expect("stored");

    let text = std::fs::read_to_string(paths::db_file(&root)).unwrap_or_default();
    // A database file is binary, so the readable parts are the ones that matter here.
    for forbidden in ["password", "token", "secret", "api_key", "credential"] {
        assert!(
            !text.to_lowercase().contains(forbidden),
            "the store holds no {forbidden}"
        );
    }

    assert!(
        !assistant.runs_on.to_lowercase().contains("key"),
        "and nothing in the record is a credential"
    );
}

#[test]
fn an_assistant_is_not_added_when_its_own_folder_cannot_be_made() {
    // A file where the folder of assistants should be, so no assistant can be given a
    // place to run.
    let root = scratch("no-home");
    std::fs::write(root.join("assistants"), "not a folder").expect("the blocker");

    let mut core = Core::open_at(root).expect("the store opened");
    let result = core.create_assistant("Nowhere", "no place to run", "");

    assert!(
        result.is_err(),
        "an assistant with nowhere to run is not made"
    );
    assert!(
        core.assistants().expect("read").is_empty(),
        "and no assistant is left in the list"
    );
}

#[test]
fn a_store_that_cannot_be_opened_says_so_rather_than_starting_anyway() {
    let blocked = scratch("store-blocked").join("in-the-way");
    std::fs::write(&blocked, "not a folder").expect("the blocker");

    assert!(
        Core::open_at(blocked).is_err(),
        "there is no smaller way to run than not running"
    );
}
