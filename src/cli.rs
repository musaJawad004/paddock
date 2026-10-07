//! Command-line interface, defined with clap derive.
//!
//! Commands:
//! - `paddock`: open the TUI (starts the daemon first from v0.2).
//! - `paddock add <dir>` / `remove <dir>`: edit the project list in the
//!   global config.
//! - `paddock list`: print projects and their detected processes.
//! - `paddock up [project]` / `down [project]`: start or stop without the TUI.
//! - `paddock daemon`: run the supervisor in the foreground (hidden; the TUI
//!   spawns it).
//!
//! Owns argument parsing and exit codes only. Help text follows
//! `.claude/rules/writing-style.md`.
