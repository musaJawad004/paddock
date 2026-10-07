//! Message types between the TUI and whatever owns the processes (the demo
//! backend today, the daemon from v0.1 on). Serialized as JSON lines once a
//! socket carries them in v0.2.
//!
//! `Output` lines are raw terminal text and may contain ANSI escape codes;
//! the TUI interprets them.
//!
//! Still to come: Hello { version }, project add and remove, StartAll and
//! StopAll, Resize, SendInput for attach mode, Shutdown.

use serde::{Deserialize, Serialize};

use crate::model::{ListeningPort, ProcessId, ProcessState, ResourceUsage, Snapshot};

pub const PROTOCOL_VERSION: u32 = 3;

/// TUI to backend.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Request {
    Start(ProcessId),
    Stop(ProcessId),
    Restart(ProcessId),
    /// SIGKILL the process group without the graceful SIGTERM first.
    Kill(ProcessId),
    /// Change the port a process should use. Restarts it if it is running.
    SetPort {
        id: ProcessId,
        port: u16,
    },
    /// Move a process to another project, creating the project if needed.
    Move {
        id: ProcessId,
        project: String,
    },
    /// Kill a listener Paddock did not start. The TUI asks the user first.
    KillPort {
        port: u16,
        pid: u32,
    },
    /// Add a project folder to config.toml and detect what it runs.
    AddProject(std::path::PathBuf),
    /// Remove a project (by its shown name) from config.toml.
    RemoveProject(String),
}

/// Backend to TUI.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Event {
    /// Full state. Sent on connect and after changes that reshape the tree.
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
    /// A process got a new id after `Move`; logs and selection follow it.
    Renamed {
        from: ProcessId,
        to: ProcessId,
    },
    /// Something the user should read, e.g. why a request was refused.
    Notice(String),
}
