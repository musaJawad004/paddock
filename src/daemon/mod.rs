//! The side of Paddock that owns child processes.
//!
//! v0.1 runs it as a task inside the TUI process. v0.2 runs the same code in
//! `paddock daemon`, a background process the TUI connects to, so servers
//! keep running after the TUI quits. Either way it is driven only by
//! `ipc::Request` and reports only through `ipc::Event`.
//!
//! Safety rules for everything in here: `.claude/rules/process-safety.md`.

pub mod logs;
pub mod ports;
pub mod pty;
pub mod stats;
pub mod supervisor;
