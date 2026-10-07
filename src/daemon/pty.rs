//! Spawns one command in its own pseudo-terminal via `portable-pty`, so dev
//! servers see a real terminal and keep their colours, spinners and QR codes.
//!
//! The command runs through the user's login shell (`$SHELL -lc <cmd>`) so
//! PATH and version managers (nvm, fnm, asdf, mise) work. The command text
//! comes only from project files or paddock.toml; nothing Paddock computed is
//! spliced into it.
//!
//! The child leads a new session and process group. Output is read on a
//! blocking thread and sent as byte chunks over a channel; resize requests
//! from the TUI are forwarded to the PTY.
