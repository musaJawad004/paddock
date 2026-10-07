//! Command-line interface, defined with clap derive.
//!
//! Today `paddock` with no arguments opens the TUI on demo data, and clap
//! provides `--help` and `--version`.
//!
//! Planned commands: `add <dir>` and `remove <dir>` to edit the project list,
//! `list`, `up [project]` and `down [project]` without the TUI, and a hidden
//! `daemon` that runs the supervisor (v0.2). Help text follows
//! `.claude/rules/writing-style.md`.

use clap::Parser;

use crate::{demo, ipc, tui};

#[derive(Debug, Parser)]
#[command(
    name = "paddock",
    version,
    about = "A terminal workspace for your dev servers",
    long_about = "A terminal workspace for your dev servers.\n\n\
        Run `paddock` to open the dashboard. This build shows demo data: \
        nothing is started on your machine yet."
)]
pub struct Cli {}

pub async fn run() -> color_eyre::Result<()> {
    let _cli = Cli::parse();

    let (client, server) = ipc::transport::in_process();
    let backend = tokio::spawn(demo::run(server));
    let result = tui::run(client, "demo data").await;
    backend.abort();
    Ok(result?)
}
