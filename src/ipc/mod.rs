//! The only way the TUI and the daemon talk.
//!
//! `protocol` defines the messages. `transport` moves them: an in-process
//! channel in v0.1, a Unix socket at `$XDG_RUNTIME_DIR/paddock.sock` (or the
//! macOS temp dir) in v0.2. Neither side knows which transport is in use.

pub mod protocol;
pub mod transport;
