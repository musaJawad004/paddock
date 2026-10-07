//! Paddock: a terminal dashboard for the dev servers running on your
//! machine. It watches; it never starts anything.
//!
//! Dependency direction (enforced by review, see `docs/ARCHITECTURE.md`):
//!
//! ```text
//! cli ──▶ tui ─────┐   (tui also reads and writes config)
//!   ├───▶ demo ────┤
//!   └───▶ monitor ─┼──▶ ipc ──▶ model
//!          └──▶ project
//! ```
//!
//! `tui` and `monitor` never import each other. They only exchange `ipc`
//! messages, so the monitor can move into a background process later
//! without touching the TUI. `demo` stands in for `monitor` with made-up
//! data.

pub mod cli;
pub mod config;
pub mod demo;
pub mod ipc;
pub mod model;
pub mod monitor;
pub mod project;
pub mod tui;
