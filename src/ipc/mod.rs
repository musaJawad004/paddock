//! The only way the TUI and the monitor talk.
//!
//! `protocol` defines the messages. `transport` moves them: today a pair of
//! in-process channels. If the monitor ever moves into a background
//! process, a Unix socket can replace the channels without either side
//! noticing.

pub mod protocol;
pub mod transport;
