//! Types shared by every other module. Plain data, no I/O, no async.
//!
//! - `ProjectId`, `ProcessId`: stable ids (project path, then process name).
//! - `Project`: folder, display name, its `ProcessSpec`s.
//! - `ProcessSpec`: name, command, working dir, env, expected port,
//!   `depends_on`, `RestartPolicy`, and where it came from (`Source`:
//!   detected from package.json, Cargo.toml, compose, Procfile, or written
//!   in paddock.toml).
//! - `ProcessState`: Stopped, Starting, Running, Stopping, Exited(code),
//!   Crashed(code or signal).
//! - `ListeningPort`: port, protocol, pid, owning `ProcessId` if known.
//! - `ResourceUsage`: CPU percent and resident memory per process.
//!
//! Everything that crosses `ipc` derives `Serialize` and `Deserialize`.
//! Changing a type here changes the wire format; bump `ipc::PROTOCOL_VERSION`.
