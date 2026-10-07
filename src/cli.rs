//! Command-line interface, defined with clap derive.
//!
//! Today `paddock` opens the TUI on demo data. Flags: `--no-splash`,
//! `--theme <name>` for one run, `--config-path` to print where settings
//! live. clap adds `--help` and `--version`.
//!
//! Planned commands: `add <dir>` and `remove <dir>` to edit the project list,
//! `list`, `up [project]` and `down [project]` without the TUI, and a hidden
//! `daemon` that runs the supervisor (v0.2). Help text follows
//! `.claude/rules/writing-style.md`.

use clap::Parser;

use crate::tui::theme::PALETTES;
use crate::{config, demo, ipc, tui};

#[derive(Debug, Parser)]
#[command(
    name = "paddock",
    version,
    about = "A terminal workspace for your dev servers",
    long_about = "A terminal workspace for your dev servers.\n\n\
        Run `paddock` to open the dashboard. This build shows demo data: \
        nothing is started on your machine yet. Press ? inside for keys, \
        and , for settings."
)]
pub struct Cli {
    /// Skip the start-up animation this time.
    #[arg(long)]
    no_splash: bool,

    /// Use this theme for this run only. Settings (,) change it for good.
    #[arg(long, value_name = "NAME", value_parser = theme_name)]
    theme: Option<String>,

    /// Print where the config file lives and exit.
    #[arg(long)]
    config_path: bool,
}

fn theme_name(name: &str) -> Result<String, String> {
    if PALETTES.iter().any(|p| p.name == name) {
        return Ok(name.to_owned());
    }
    let names: Vec<&str> = PALETTES.iter().map(|p| p.name).collect();
    Err(format!("choose one of: {}", names.join(", ")))
}

pub async fn run() -> color_eyre::Result<()> {
    let cli = Cli::parse();

    if cli.config_path {
        println!("{}", config::path()?.display());
        return Ok(());
    }

    let (mut config, notice) = match config::load() {
        Ok(config) => (config, None),
        Err(err) => (
            config::Config::default(),
            Some(format!("{err}. Using defaults.")),
        ),
    };
    if let Some(theme) = cli.theme {
        config.ui.theme = theme;
    }
    let options = tui::Options {
        source: "demo data".into(),
        splash: config.ui.splash && !cli.no_splash,
        config,
        notice,
    };

    let (client, server) = ipc::transport::in_process();
    let backend = tokio::spawn(demo::run(server));
    let result = tui::run(client, options).await;
    backend.abort();
    Ok(result?)
}
