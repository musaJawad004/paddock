//! Types shared by every other module. Plain data, no I/O, no async.
//!
//! Everything here crosses `ipc`, so it all derives `Serialize` and
//! `Deserialize`. Changing a type changes the wire format; bump
//! `ipc::protocol::PROTOCOL_VERSION` when you do.
//!
//! Still to come with `detect` and `config`: `ProcessSpec` (command, cwd,
//! env, `depends_on`, restart policy, where it was detected from).

use std::fmt;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// A process is named by its project and its own name, e.g. `storefront/web`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProcessId {
    pub project: String,
    pub name: String,
}

impl ProcessId {
    pub fn new(project: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            project: project.into(),
            name: name.into(),
        }
    }
}

impl fmt::Display for ProcessId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.project, self.name)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProcessState {
    Stopped,
    Starting,
    Running,
    Stopping,
    /// Exited on its own with status 0.
    Exited,
    /// Exited with a non-zero status, or was killed by a signal (`None`).
    Crashed(Option<i32>),
}

impl ProcessState {
    pub fn is_up(self) -> bool {
        matches!(self, Self::Starting | Self::Running)
    }

    pub fn label(self) -> String {
        match self {
            Self::Stopped => "stopped".into(),
            Self::Starting => "starting".into(),
            Self::Running => "running".into(),
            Self::Stopping => "stopping".into(),
            Self::Exited => "exited".into(),
            Self::Crashed(Some(code)) => format!("crashed (exit {code})"),
            Self::Crashed(None) => "crashed (signal)".into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ResourceUsage {
    pub cpu_percent: f32,
    pub memory_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProcessInfo {
    pub id: ProcessId,
    pub command: String,
    pub state: ProcessState,
    /// The port it listens on once running, or the expected one as a hint.
    pub port: Option<u16>,
    pub usage: Option<ResourceUsage>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectInfo {
    pub name: String,
    pub path: PathBuf,
    pub processes: Vec<ProcessInfo>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ListeningPort {
    pub port: u16,
    pub pid: u32,
    pub command: String,
    /// `None` when the listener was not started by Paddock.
    pub owner: Option<ProcessId>,
}

/// Everything the TUI needs to draw, sent once on connect.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub projects: Vec<ProjectInfo>,
    pub ports: Vec<ListeningPort>,
}
