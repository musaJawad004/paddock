//! Message types between the TUI and whatever watches the servers (the
//! monitor, or the demo). Serialized as JSON lines once a socket carries
//! them.

use serde::{Deserialize, Serialize};

use crate::model::{ServerId, Snapshot};

pub const PROTOCOL_VERSION: u32 = 6;

/// TUI to monitor. Every request acts on something already running; the
/// TUI confirms with the user first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Request {
    /// SIGTERM to every process in the server's tree, SIGKILL after 5 s.
    Stop(ServerId),
    /// SIGKILL to every process in the tree, now.
    Kill(ServerId),
    /// Stream this server's log (or none). Replaces the previous one.
    Follow(Option<ServerId>),
}

/// Monitor to TUI.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Event {
    /// Full state after every scan, about every two seconds.
    Snapshot(Snapshot),
    /// Lines from the followed server's log. `reset` starts the view over
    /// (a new server, or the file was replaced).
    Logs {
        id: ServerId,
        lines: Vec<String>,
        reset: bool,
    },
    /// Something the user should read, e.g. the result of a stop.
    Notice(String),
}
