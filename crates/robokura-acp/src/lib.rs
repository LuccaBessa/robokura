//! Robokura's client for the Agent Client Protocol.
//!
//! The agent brings its own filesystem, browser, and shell. Nothing here advertises a
//! client filesystem or terminal, so an agent has no route through this application
//! to the person's files. The model never sees a credential: the agent keeps its own
//! sign-in and this crate holds none.
//!
//! Nothing here knows what an assistant or a thread is. Those belong to
//! `robokura-core`, which is why this crate can be read on its own.

pub mod agents;
pub mod command;
pub mod session;

pub use session::{Choice, Event, Selector, Session, Start};
