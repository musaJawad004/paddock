//! Output history for one process.
//!
//! - A `vt100::Parser` sized like the TUI log pane, so cursor movement and
//!   redraws (progress bars, Expo's QR code) render correctly.
//! - A bounded ring of plain-text lines (default 10,000) for scrollback and
//!   search. Memory stays flat no matter how long a server runs.
//! - Marks lines that look like errors (`error`, `ERR!`, `panicked at`,
//!   stack frames) so the TUI can highlight them and count them.
//!
//! Never writes logs to disk and never logs environment variables.
