//! Types shared by every other module. Plain data, no I/O, no async.
//!
//! Paddock watches dev servers; it never starts them. A `Server` is a
//! process tree the user started somewhere (a terminal, an editor) that
//! listens on at least one port, inside a folder below their home folder.
//! Servers are grouped by `Project`, the folder they run in.
//!
//! Everything here crosses `ipc`, so it all derives `Serialize` and
//! `Deserialize`. Changing a type changes the wire format; bump
//! `ipc::protocol::PROTOCOL_VERSION` when you do.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// A server is named by the pid at the top of its process tree plus that
/// process's start time, because pids get reused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ServerId {
    pub pid: u32,
    /// Unix seconds.
    pub started: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ServerState {
    Running,
    /// Asked to stop; still there.
    Stopping,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ResourceUsage {
    pub cpu_percent: f32,
    pub memory_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Server {
    pub id: ServerId,
    /// Short name, e.g. the npm script ("dev") or the program ("vite").
    pub name: String,
    /// Full command line of the top process.
    pub command: String,
    pub cwd: PathBuf,
    pub ports: Vec<u16>,
    /// Processes in its tree, the top one included.
    pub processes: usize,
    /// Summed over the whole tree.
    pub usage: ResourceUsage,
    pub state: ServerState,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub name: String,
    pub path: PathBuf,
    /// "node", "rust"... from the project's files, or "folder".
    pub kind: String,
    pub servers: Vec<Server>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ListeningPort {
    pub port: u16,
    pub pid: u32,
    pub command: String,
    pub owner: ServerId,
}

/// Everything the TUI draws, sent after every scan.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub projects: Vec<Project>,
    pub ports: Vec<ListeningPort>,
}

impl Snapshot {
    pub fn servers(&self) -> impl Iterator<Item = &Server> {
        self.projects.iter().flat_map(|p| &p.servers)
    }
}

/// Unix time in milliseconds.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}
