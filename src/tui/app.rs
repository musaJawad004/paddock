//! TUI state and the event loop.
//!
//! - `App`: latest snapshot from the daemon, selection, focused pane, scroll
//!   and search state, open popup, `dirty` flag.
//! - `Msg`: Key, Resize, Tick, Daemon(Event).
//! - `update(&mut App, Msg) -> Option<Request>`: all state changes happen
//!   here, so they can be unit tested without a terminal.
//! - The loop `select!`s over terminal events, daemon events and a redraw
//!   tick, and draws only when `dirty`, at most ~30 times a second.
//!
//! Terminal setup and teardown go through `ratatui::init` and
//! `ratatui::restore` so a panic never leaves the shell broken.
