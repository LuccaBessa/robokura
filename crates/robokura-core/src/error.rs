//! What went wrong, said in words a person can read.

use std::fmt;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    Sqlite(rusqlite::Error),
    Io(std::io::Error),
    /// This machine has nowhere this application may keep files.
    NowhereToKeepFiles,
    /// Asked for something that is not there.
    NotFound(String),
    /// The assistant's agent was still holding its own folder.
    ///
    /// Said in its own words, because the operating system's complaint is about a file
    /// being in use and someone removing an assistant has no file in mind. The
    /// underlying error is kept for whoever is reading a log.
    AgentStillRunning(std::io::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Sqlite(error) => write!(f, "the records could not be read or written: {error}"),
            Error::Io(error) => write!(f, "a file could not be written: {error}"),
            Error::NowhereToKeepFiles => {
                write!(f, "this machine has no place Robokura can keep its files")
            }
            Error::NotFound(what) => write!(f, "{what} was not found"),
            Error::AgentStillRunning(_) => write!(
                f,
                "the agent is still running and is holding its own folder, so the \
                 agent could not be removed. It is still here; try again in a moment."
            ),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Sqlite(error) => Some(error),
            Error::Io(error) => Some(error),
            Error::AgentStillRunning(error) => Some(error),
            _ => None,
        }
    }
}

impl From<rusqlite::Error> for Error {
    fn from(error: rusqlite::Error) -> Self {
        Error::Sqlite(error)
    }
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Error::Io(error)
    }
}
