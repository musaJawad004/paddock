//! Types shared by every other module. Plain data, no I/O, no async.
//!
//! Everything here crosses `ipc`, so it all derives `Serialize` and
//! `Deserialize`. Changing a type changes the wire format; bump
//! `ipc::protocol::PROTOCOL_VERSION` when you do.
//!
//! Still to come: `depends_on` and a restart policy on `ProcessSpec`.

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

/// How to run one process, as found by `detect` or written in paddock.toml.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessSpec {
    pub name: String,
    /// Shell command, run through the user's login shell.
    pub command: String,
    pub cwd: PathBuf,
    /// Port the tool listens on by default, as a hint until it really opens.
    pub port: Option<u16>,
    /// Arguments that move it to another port, with `{port}` as the
    /// placeholder, e.g. `--port {port}`. `None` when only `PORT` works.
    pub port_args: Option<String>,
    pub env: Vec<(String, String)>,
    /// Where it was found, e.g. `package.json script "dev"`.
    pub source: String,
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
            Self::Crashed(None) => "killed".into(),
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
    /// Working directory the command runs in.
    pub cwd: PathBuf,
    /// Where the command came from, e.g. "package.json script dev".
    pub source: String,
    pub pid: Option<u32>,
    /// When it last started, in Unix milliseconds.
    pub started_at_ms: Option<u64>,
    pub restarts: u32,
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

/// A dev server running on this machine that Paddock did not start, in a
/// project folder that is not in Paddock yet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Discovered {
    pub name: String,
    pub path: PathBuf,
    /// "node", "rust"... from the project's files.
    pub kind: String,
    pub ports: Vec<u16>,
    pub pid: u32,
    pub command: String,
}

/// Everything the TUI needs to draw, sent once on connect.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub projects: Vec<ProjectInfo>,
    pub ports: Vec<ListeningPort>,
    pub discovered: Vec<Discovered>,
}

/// Unix time in milliseconds. Timestamps cross `ipc`, so they are plain
/// numbers rather than `Instant`s.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}
