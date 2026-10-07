//! Message types, serialized as JSON lines.
//!
//! - `Request` (TUI to daemon): Hello { version }, ListProjects, AddProject,
//!   RemoveProject, Start, Stop, Restart, StartAll, StopAll, Resize,
//!   Subscribe { process }, SendInput { process, bytes }, KillForeignPort
//!   { port, pid }, Shutdown.
//! - `Event` (daemon to TUI): Snapshot (full state on connect), ProcessState,
//!   Output { process, bytes }, Ports, Usage, Error { message }.
//!
//! `PROTOCOL_VERSION` is checked in Hello; a mismatch tells the user to run
//! `paddock down` and reopen, instead of misreading messages.
