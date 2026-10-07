//! Command-line interface, defined with clap derive.
//!
//! - `paddock`: the dashboard of every dev server running on this machine.
//! - `paddock list`: the same list printed once, without the dashboard.
//! - `paddock --demo`: the dashboard on made-up servers.
//!
//! Flags: `--no-splash`, `--theme <name>` for one run, `--config-path` to
//! print where settings live. Help text follows
//! `.claude/rules/writing-style.md`.

use std::collections::HashSet;
use std::time::Duration;

use clap::{Parser, Subcommand};
use color_eyre::eyre::eyre;

use crate::monitor::stats::Stats;
use crate::tui::theme::PALETTES;
use crate::{config, demo, ipc, monitor, tui};

#[derive(Debug, Parser)]
#[command(
    name = "paddock",
    version,
    about = "Every dev server on this machine, in one place",
    long_about = "Every dev server on this machine, in one place.\n\n\
        Paddock finds the servers you start in any terminal, groups them by \
        project, and lets you open, copy, stop or kill them. It never starts \
        anything and has no network code. Inside, press ? for keys and , for \
        settings."
)]
pub struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// Skip the start-up animation this time.
    #[arg(long, global = true)]
    no_splash: bool,

    /// Use this theme for this run only. Settings (,) change it for good.
    #[arg(long, value_name = "NAME", value_parser = theme_name, global = true)]
    theme: Option<String>,

    /// Show the dashboard with made-up servers.
    #[arg(long)]
    demo: bool,

    /// Print where the config file lives and exit.
    #[arg(long)]
    config_path: bool,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Print the running dev servers once and exit.
    List,
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
    match cli.command {
        Some(Command::List) => list(),
        None => dashboard(cli.demo, cli.no_splash, cli.theme).await,
    }
}

async fn dashboard(demo: bool, no_splash: bool, theme: Option<String>) -> color_eyre::Result<()> {
    let (mut config, notice) = match config::load() {
        Ok(config) => (config, None),
        Err(err) => (
            config::Config::default(),
            Some(format!("{err}. Using defaults.")),
        ),
    };
    if let Some(theme) = theme {
        config.ui.theme = theme;
    }
    let options = tui::Options {
        source: if demo { "demo data" } else { "local" }.into(),
        splash: config.ui.splash && !no_splash,
        notice,
        config,
    };

    let (client, server) = ipc::transport::in_process();
    let watcher = if demo {
        tokio::spawn(demo::run(server))
    } else {
        tokio::spawn(monitor::run(server))
    };
    let result = tui::run(client, options).await;
    let _ = tokio::time::timeout(Duration::from_secs(2), watcher).await;
    Ok(result?)
}

fn list() -> color_eyre::Result<()> {
    let home = monitor::home().ok_or_else(|| eyre!("HOME is not set"))?;
    let mut stats = Stats::new();
    let snapshot = monitor::scan(&mut stats, &home, &HashSet::new()).map_err(|e| eyre!(e))?;
    if snapshot.projects.is_empty() {
        println!("No dev servers running.");
    }
    for (i, project) in snapshot.projects.iter().enumerate() {
        if i > 0 {
            println!();
        }
        println!(
            "{} ({}, {})",
            project.name,
            tui::tilde(&project.path),
            project.kind
        );
        for server in &project.servers {
            let ports: Vec<String> = server.ports.iter().map(|p| format!(":{p}")).collect();
            println!(
                "  {:<14} {:<14} pid {:<7} {}",
                server.name,
                ports.join(" "),
                server.id.pid,
                server.command
            );
        }
    }
    let foreign: Vec<String> = snapshot
        .ports
        .iter()
        .filter(|p| p.owner.is_none())
        .map(|p| format!(":{} {} (pid {})", p.port, p.command, p.pid))
        .collect();
    if !foreign.is_empty() {
        println!("\nOther programs on dev ports: {}", foreign.join(", "));
    }
    Ok(())
}
