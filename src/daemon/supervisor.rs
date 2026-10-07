//! Process lifecycle: the state machine in `docs/ARCHITECTURE.md`.
//!
//! - Start: resolve `depends_on` order, spawn through `pty`, move to
//!   Starting, then Running once output or the expected port is seen.
//! - Stop: SIGTERM to the process group, wait (5 s default), SIGKILL.
//! - Exit: record the code; non-zero is Crashed. `RestartPolicy::OnFailure`
//!   restarts with backoff 1 s, 2 s, 4 s, up to 30 s, reset after 60 s up.
//! - Shutdown (`paddock down` or TUI quit in v0.1): stop everything, wait for
//!   every group to be gone.
//!
//! Tracks each child by pid and start time, because pids are reused. Never
//! signals a process it did not start.
