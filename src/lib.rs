//! Paddock: a terminal workspace for dev servers.
//!
//! Dependency direction (enforced by review, see `docs/ARCHITECTURE.md`):
//!
//! ```text
//! cli ──▶ tui ─────┐
//!   └───▶ daemon ──┼──▶ ipc ──▶ model
//!          └──▶ detect ───────▶ model
//! config ─────────────────────▶ model
//! ```
//!
//! `tui` and `daemon` never import each other. They only exchange
//! `ipc` messages, which is what lets v0.2 move the daemon into its own
//! process without touching the TUI.

pub mod cli;
pub mod config;
pub mod daemon;
pub mod detect;
pub mod ipc;
pub mod model;
pub mod tui;
