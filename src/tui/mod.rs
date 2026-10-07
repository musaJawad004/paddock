//! The terminal UI. Holds no process handles: everything it knows arrives as
//! `ipc::Event`, everything it does leaves as `ipc::Request`.
//!
//! Built the way `.claude/skills/ratatui-tui/SKILL.md` describes: state in
//! `app`, pure update functions, one view module per screen area, keys in
//! one table, colours in one theme.

pub mod app;
pub mod keys;
pub mod logs_view;
pub mod palette;
pub mod sidebar;
pub mod status_bar;
pub mod theme;
