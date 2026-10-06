//! What the person has configured for this machine, and the parts of that page.
//!
//! This file only says what is in the folder.

pub mod chooser;
pub mod settings_pane;

pub use chooser::{about, ordered};
pub use settings_pane::SettingsPane;
