//! The middle region: the thread of the assistant that is open, and the parts of it.
//!
//! This file only says what is in the folder.

pub mod composer;
pub mod thread_pane;
pub mod transcript_item;

pub use thread_pane::ThreadPane;
