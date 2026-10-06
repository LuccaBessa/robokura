//! The window.
//!
//! Every part is a component in a file of its own, named for what it is, grouped into
//! a folder named for the region of the window it sits in. A folder's `mod.rs` only
//! says what is in it, so the file a component lives in is the only place that
//! component is written.
//!
//! The window's state is [`AppView`]. It owns the three panes, decides which of them is
//! in front, and builds each pane when it is asked for rather than while it is being
//! drawn.

pub mod app_view;
pub mod bar;
pub mod details;
pub mod layout;
pub mod main_thread;
pub mod settings;
pub mod sidebar;

pub use app_view::{AppView, open};
