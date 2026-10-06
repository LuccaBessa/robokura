//! Where everything is written.
//!
//! Every path here is built from a root the caller supplies. Nothing in this crate
//! reaches for a fixed location on its own, which is what lets the checks run against
//! a scratch folder instead of a person's own assistants.

use std::path::{Path, PathBuf};

/// `LOCALAPPDATA` on Windows, `~/Library/Application Support` on macOS, and
/// `~/.local/share` elsewhere.
pub fn base_dir() -> Option<PathBuf> {
    if cfg!(target_os = "windows") {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|home| {
            PathBuf::from(home)
                .join("Library")
                .join("Application Support")
        })
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME")
                    .map(|home| PathBuf::from(home).join(".local").join("share"))
            })
    }
}

pub fn root() -> Option<PathBuf> {
    base_dir().map(|base| base.join("Robokura"))
}

pub fn db_file(root: &Path) -> PathBuf {
    root.join("robokura.db")
}

/// Also where the agent is started. Starting every assistant in the same folder would
/// let one's files be found by another's.
pub fn assistant_home(root: &Path, assistant_id: &str) -> PathBuf {
    root.join("assistants").join(assistant_id)
}
