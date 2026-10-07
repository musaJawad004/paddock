//! Loads and saves configuration. Never runs commands.
//!
//! Global: `$XDG_CONFIG_HOME/paddock/config.toml`, falling back to
//! `~/.config/paddock/config.toml` on macOS and Linux alike, which is where
//! terminal tools are expected to keep config. Holds the list of project
//! folders, scrollback size and theme override. Created on first
//! `paddock add`.
//!
//! Per project: optional `paddock.toml` in the project root. Its processes
//! replace detected ones with the same name and add new ones.
//!
//! Unknown keys are ignored with a warning in the log, so older Paddock
//! versions can read newer configs. Paths starting with `~` are expanded here
//! and nowhere else.
