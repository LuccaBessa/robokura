//! The database.
//!
//! One file holds everything. It is opened once, kept for the life of the
//! application, and every statement is short enough to read in place.
//!
//! The shape is carried in migrations rather than in the file, so a database written
//! by an older version is brought forward one step at a time and a step that fails
//! leaves the file as it was.

use std::collections::BTreeMap;
use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};

use crate::domain::{Assistant, ConfigOption, Kind, Message, Preview, Settings, Thread};
use crate::{Error, Result};

/// The table is called agent here and renamed to assistant by the last migration,
/// because the name it was first written under is the one the protocol uses for
/// something else.
const FIRST: &str = "
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

CREATE INDEX message_by_thread ON message(thread_id, seq);
";

/// One row per key rather than one column per setting, so a setting added later is a
/// second row and not a change to a table three checks read from.
const SETTINGS: &str = "
CREATE TABLE setting (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
";

/// Renaming a table rewrites the foreign keys that point at it, so the columns holding
/// the reference are renamed after rather than before, and the settings key moves with
/// them so a chosen agent is not lost by being called something else.
const ASSISTANT: &str = "
ALTER TABLE agent RENAME TO assistant;

ALTER TABLE assistant RENAME COLUMN program TO runs_on;

ALTER TABLE thread RENAME COLUMN agent_id TO assistant_id;

ALTER TABLE message RENAME COLUMN agent_id TO assistant_id;

UPDATE setting SET key = 'runs_on' WHERE key = 'program';
";

/// The position in this list is the version it brings the database to, so a step is
/// only ever added at the end.
const MIGRATIONS: &[&str] = &[FIRST, SETTINGS, ASSISTANT];

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        // Write-ahead logging so a read is never blocked by a write, and foreign keys
        // on so deleting an assistant takes its thread with it.
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.pragma_update(None, "busy_timeout", 5_000)?;
        Self::migrate(&conn)?;
        Ok(Self { conn })
    }

    fn migrate(conn: &Connection) -> Result<()> {
        let current: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
        for (index, sql) in MIGRATIONS.iter().enumerate() {
            let target = index as i64 + 1;
            if current >= target {
                continue;
            }
            let step = conn.unchecked_transaction()?;
            step.execute_batch(sql)?;
            step.pragma_update(None, "user_version", target)?;
            step.commit()?;
        }
        Ok(())
    }

    // Assistants.

    /// Two writes that have to both happen or neither, so the assistant is never
    /// listed with no thread behind it.
    pub fn add_assistant(&self, assistant: &Assistant) -> Result<()> {
        let thread = Thread {
            id: uuid::Uuid::new_v4().simple().to_string(),
            assistant_id: assistant.id.clone(),
            created_at: assistant.created_at,
            updated_at: assistant.created_at,
        };
        let step = self.conn.unchecked_transaction()?;
        step.execute(
            "INSERT INTO assistant (id, name, title, description, runs_on, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                assistant.id,
                assistant.name,
                assistant.title,
                assistant.description,
                assistant.runs_on,
                assistant.created_at
            ],
        )?;
        step.execute(
            "INSERT INTO thread (id, assistant_id, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                thread.id,
                thread.assistant_id,
                thread.created_at,
                thread.updated_at
            ],
        )?;
        step.commit()?;
        Ok(())
    }

    /// Newest said first, so the thread somebody was last speaking in is the one at the
    /// top of the list. The time is the one a row shows, and an assistant with nothing
    /// said yet is placed by when it was made. Times are in seconds, so the rowid settles
    /// anything two assistants share and the one made last comes first.
    pub fn assistants(&self) -> Result<Vec<Assistant>> {
        let mut statement = self.conn.prepare(
            "SELECT a.id, a.name, a.title, a.description, a.runs_on, a.created_at
             FROM assistant a
             LEFT JOIN thread t ON t.assistant_id = a.id
             LEFT JOIN message m ON m.id = (
                 SELECT x.id FROM message x
                 WHERE x.thread_id = t.id AND x.body <> ''
                 ORDER BY x.seq DESC
                 LIMIT 1
             )
             ORDER BY COALESCE(m.created_at, a.created_at) DESC, a.rowid DESC",
        )?;
        let rows = statement.query_map([], read_assistant)?;
        collect(rows)
    }

    /// The name already on past messages is left alone, so renaming an assistant does
    /// not rewrite what it said.
    pub fn update_assistant(&self, assistant: &Assistant) -> Result<bool> {
        let changed = self.conn.execute(
            "UPDATE assistant SET name = ?2, title = ?3, description = ?4 WHERE id = ?1",
            params![
                assistant.id,
                assistant.name,
                assistant.title,
                assistant.description
            ],
        )?;
        Ok(changed > 0)
    }

    pub fn assistant(&self, id: &str) -> Result<Option<Assistant>> {
        self.conn
            .query_row(
                "SELECT id, name, title, description, runs_on, created_at
                 FROM assistant WHERE id = ?1",
                params![id],
                read_assistant,
            )
            .optional()
            .map_err(Error::from)
    }

    pub fn delete_assistant(&self, id: &str) -> Result<bool> {
        let removed = self
            .conn
            .execute("DELETE FROM assistant WHERE id = ?1", params![id])?;
        Ok(removed > 0)
    }

    // Threads.

    pub fn thread_for(&self, assistant_id: &str) -> Result<Option<Thread>> {
        self.conn
            .query_row(
                "SELECT id, assistant_id, created_at, updated_at
                 FROM thread WHERE assistant_id = ?1",
                params![assistant_id],
                |row| {
                    Ok(Thread {
                        id: row.get(0)?,
                        assistant_id: row.get(1)?,
                        created_at: row.get(2)?,
                        updated_at: row.get(3)?,
                    })
                },
            )
            .optional()
            .map_err(Error::from)
    }

    fn touch(&self, thread_id: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE thread SET updated_at = ?2 WHERE id = ?1",
            params![thread_id, now()],
        )?;
        Ok(())
    }

    // Messages.

    pub fn messages(&self, thread_id: &str) -> Result<Vec<Message>> {
        let mut statement = self.conn.prepare(
            "SELECT id, thread_id, seq, kind, assistant_id, author_name, body,
                    created_at, updated_at, complete
             FROM message WHERE thread_id = ?1 ORDER BY seq",
        )?;
        let rows = statement.query_map(params![thread_id], read_message)?;
        collect(rows)
    }

    pub fn add_message(&self, message: &Message) -> Result<()> {
        self.conn.execute(
            "INSERT INTO message
                 (id, thread_id, seq, kind, assistant_id, author_name, body,
                  created_at, updated_at, complete)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                message.id,
                message.thread_id,
                message.seq,
                message.kind.as_str(),
                message.assistant_id,
                message.author_name,
                message.body,
                message.created_at,
                message.updated_at,
                message.complete
            ],
        )?;
        self.touch(&message.thread_id)
    }

    /// On an interval rather than on every piece of text, because a reply arrives
    /// faster than a window should be redrawn.
    pub fn write_body(&self, id: &str, body: &str, complete: bool) -> Result<()> {
        self.conn.execute(
            "UPDATE message SET body = ?2, complete = ?3, updated_at = ?4 WHERE id = ?1",
            params![id, body, complete, now()],
        )?;
        Ok(())
    }

    /// From what is already stored rather than from a clock, so a reply that is still
    /// arriving holds its place and nothing else can take it.
    pub fn next_seq(&self, thread_id: &str) -> Result<i64> {
        Ok(self.conn.query_row(
            "SELECT COALESCE(MAX(seq), 0) + 1 FROM message WHERE thread_id = ?1",
            params![thread_id],
            |row| row.get(0),
        )?)
    }

    /// The last thing said in each thread that has anything said in it. A reply's row
    /// is stored before the agent has said a word, so a mid-answer thread ends with an
    /// empty body, and those are passed over for the same reason the conversation
    /// leaves them out.
    pub fn previews(&self) -> Result<Vec<Preview>> {
        let mut statement = self.conn.prepare(
            "SELECT t.assistant_id, substr(m.body, 1, ?1), m.created_at
             FROM thread t
             JOIN message m ON m.id = (
                 SELECT x.id FROM message x
                 WHERE x.thread_id = t.id AND x.body <> ''
                 ORDER BY x.seq DESC
                 LIMIT 1
             )",
        )?;
        let rows = statement.query_map(params![PREVIEW_CHARS], read_preview)?;
        collect(rows)
    }

    // Settings.

    /// A key that is missing or unreadable is a setting nobody has changed yet, not an
    /// error. A key this version does not know is left alone rather than removed, so
    /// that a database written by a newer version is not quietly stripped by an older
    /// one opening it.
    pub fn settings(&self) -> Result<Settings> {
        let mut statement = self.conn.prepare("SELECT key, value FROM setting")?;
        let rows = statement.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;

        let mut settings = Settings::default();
        for row in rows {
            let (key, value) = row?;
            match key.as_str() {
                "runs_on" => settings.runs_on = value,
                "chosen" => {
                    if let Ok(text) = serde_json::from_str::<serde_json::Value>(&value) {
                        settings.chosen =
                            read_map(&text, |_, value| value.as_str().map(str::to_string));
                    }
                }
                "offered" => {
                    if let Ok(text) = serde_json::from_str::<serde_json::Value>(&value) {
                        settings.offered =
                            read_map(&text, |_, options| Some(read_options(options.as_array())));
                    }
                }
                "mode" => settings.mode = value,
                _ => {}
            }
        }
        Ok(settings)
    }

    /// One transaction for every key, so a half-written set is not left behind.
    pub fn save_settings(&self, settings: &Settings) -> Result<()> {
        let rows: Vec<(&str, String)> = vec![
            ("runs_on", settings.runs_on.clone()),
            (
                "chosen",
                serde_json::to_string(&settings.chosen).unwrap_or_else(|_| "{}".to_string()),
            ),
            (
                "offered",
                serde_json::to_string(&settings.offered).unwrap_or_else(|_| "{}".to_string()),
            ),
            ("mode", settings.mode.clone()),
        ];

        let step = self.conn.unchecked_transaction()?;
        for (key, value) in rows {
            step.execute(
                "INSERT INTO setting (key, value) VALUES (?1, ?2)
                 ON CONFLICT (key) DO UPDATE SET value = excluded.value",
                params![key, value],
            )?;
        }
        step.commit()?;
        Ok(())
    }
}

/// Anything unreadable is dropped rather than refused: the next time that agent starts
/// it says again, and the person's choice is applied on top.
fn read_map<T>(
    value: &serde_json::Value,
    each: impl Fn(&str, &serde_json::Value) -> Option<T>,
) -> BTreeMap<String, T> {
    let Some(object) = value.as_object() else {
        return BTreeMap::new();
    };
    object
        .iter()
        .filter_map(|(key, value)| each(key, value).map(|read| (key.clone(), read)))
        .collect()
}

/// Deserialised rather than picked apart field by field. Reading it by hand is a second
/// description of the same type: one that can name a field this version knows and
/// quietly miss another.
fn read_options(items: Option<&Vec<serde_json::Value>>) -> Vec<ConfigOption> {
    items
        .map(|items| {
            items
                .iter()
                .filter_map(|item| serde_json::from_value(item.clone()).ok())
                .collect()
        })
        .unwrap_or_default()
}

/// A bound on what is carried for every agent on every redraw. The row ellipsises
/// whatever it is given, so nothing is added on top of this.
const PREVIEW_CHARS: i64 = 200;

fn read_assistant(row: &rusqlite::Row<'_>) -> rusqlite::Result<Assistant> {
    Ok(Assistant {
        id: row.get(0)?,
        name: row.get(1)?,
        title: row.get(2)?,
        description: row.get(3)?,
        runs_on: row.get(4)?,
        created_at: row.get(5)?,
    })
}

fn read_preview(row: &rusqlite::Row<'_>) -> rusqlite::Result<Preview> {
    Ok(Preview {
        assistant_id: row.get(0)?,
        body: row.get(1)?,
        at: row.get(2)?,
    })
}

fn read_message(row: &rusqlite::Row<'_>) -> rusqlite::Result<Message> {
    let kind: String = row.get(3)?;
    Ok(Message {
        id: row.get(0)?,
        thread_id: row.get(1)?,
        seq: row.get(2)?,
        kind: match kind.as_str() {
            "note" => Kind::Note,
            _ => Kind::Text,
        },
        assistant_id: row.get(4)?,
        author_name: row.get(5)?,
        body: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
        complete: row.get(9)?,
    })
}

fn collect<T>(rows: impl Iterator<Item = rusqlite::Result<T>>) -> Result<Vec<T>> {
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or_default()
}
