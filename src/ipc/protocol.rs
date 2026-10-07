//! Message types between the TUI and whatever owns the processes (the demo
//! backend today, the daemon from v0.1 on). Serialized as JSON lines once a
//! socket carries them in v0.2.
//!
//! Still to come: Hello { version }, project add and remove, StartAll and
//! StopAll, Resize, SendInput for attach mode, KillForeignPort, Shutdown,
//! and raw PTY bytes in place of text lines in `Output`.

use serde::{Deserialize, Serialize};

use crate::model::{ListeningPort, ProcessId, ProcessState, ResourceUsage, Snapshot};

pub const PROTOCOL_VERSION: u32 = 1;

/// TUI to backend.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Request {
    Start(ProcessId),
    Stop(ProcessId),
    Restart(ProcessId),
}

/// Backend to TUI.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Event {
    Snapshot(Snapshot),
    State {
        id: ProcessId,
        state: ProcessState,
    },
    Output {
        id: ProcessId,
        lines: Vec<String>,
    },
    Ports(Vec<ListeningPort>),
    /// Latest CPU and memory for every running process, about once a second.
    Usage(Vec<(ProcessId, ResourceUsage)>),
}
