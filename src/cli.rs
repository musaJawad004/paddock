//! Command-line interface, defined with clap derive.
//!
//! - `paddock`: the dashboard, running the projects in config.toml.
//! - `paddock add [folder]`: add a project (default: the current folder)
//!   and print what Paddock will run there.
//! - `paddock remove <name or folder>`: take a project off the list.
//! - `paddock list`: every project and its processes, without the TUI.
//! - `paddock --demo`: the dashboard on made-up data; nothing runs.
//!
//! Flags for the dashboard: `--no-splash`, `--theme <name>` for one run,
//! `--config-path` to print where settings live. Help text follows
//! `.claude/rules/writing-style.md`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use clap::{Parser, Subcommand};
use color_eyre::eyre::{WrapErr, eyre};

use crate::tui::theme::PALETTES;
use crate::{config, daemon, demo, detect, ipc, tui};

#[derive(Debug, Parser)]
#[command(
    name = "paddock",
    version,
    about = "A terminal workspace for your dev servers",
    long_about = "A terminal workspace for your dev servers.\n\n\
        Run `paddock` to open the dashboard, `paddock add` in a project folder \
        to add it. Inside, press ? for keys and , for settings. Everything \
        runs on this machine; Paddock has no network code."
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

    /// Show the dashboard with made-up projects. Nothing is started.
    #[arg(long)]
    demo: bool,

    /// Print where the config file lives and exit.
    #[arg(long)]
    config_path: bool,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Add a project folder and show what Paddock found to run in it.
    Add {
        /// The folder to add. Defaults to the current one.
        folder: Option<PathBuf>,
    },
    /// Remove a project from the list. Its files are not touched.
    Remove {
        /// The project's name (its folder name) or its folder.
        project: String,
    },
    /// List projects and the processes Paddock found in each.
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
        Some(Command::Add { folder }) => add(folder),
        Some(Command::Remove { project }) => remove(&project),
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
        stops_on_quit: !demo,
        cwd: std::env::current_dir().unwrap_or_default(),
        config: config.clone(),
    };

    let (client, server) = ipc::transport::in_process();
    let backend = if demo {
        tokio::spawn(demo::run(server))
    } else {
        let path = config::path()?;
        tokio::spawn(daemon::run(server, config, path))
    };
    let result = tui::run(client, options).await;
    // The TUI dropped its end, so the backend is now stopping every process
    // it started. Give it time to do that cleanly.
    if !demo {
        eprintln!("Stopping processes...");
    }
    let _ = tokio::time::timeout(Duration::from_secs(8), backend).await;
    Ok(result?)
}

fn add(folder: Option<PathBuf>) -> color_eyre::Result<()> {
    let folder = match folder {
        Some(folder) => config::expand_home(&folder),
        None => std::env::current_dir()?,
    };
    let path = folder
        .canonicalize()
        .wrap_err_with(|| format!("{} does not exist", folder.display()))?;
    if !path.is_dir() {
        return Err(eyre!("{} is not a folder", path.display()));
    }
    let name = detect::project_name(&path);
    let mut added = false;
    config::update(|config| {
        if !config
            .projects
            .iter()
            .any(|p| config::expand_home(p) == path)
        {
            config.projects.push(path.clone());
            added = true;
        }
    })?;
    if added {
        println!("Added {name} ({})", tui::tilde(&path));
    } else {
        println!("{name} is already in Paddock");
    }
    print_processes(&path);
    Ok(())
}

fn remove(project: &str) -> color_eyre::Result<()> {
    let as_path = config::expand_home(Path::new(project)).canonicalize().ok();
    let mut removed = None;
    config::update(|config| {
        config.projects.retain(|p| {
            let p = config::expand_home(p);
            let matches = Some(&p) == as_path.as_ref() || detect::project_name(&p) == project;
            if matches {
                removed = Some(p);
            }
            !matches
        });
    })?;
    match removed {
        Some(path) => println!(
            "Removed {} from Paddock. Its files are untouched.",
            tui::tilde(&path)
        ),
        None => println!("{project} is not in Paddock. `paddock list` shows what is."),
    }
    Ok(())
}

fn list() -> color_eyre::Result<()> {
    let config = config::load()?;
    if config.projects.is_empty() {
        println!("No projects yet. Run `paddock add` in a project folder.");
        return Ok(());
    }
    for (i, project) in config.projects.iter().enumerate() {
        let path = config::expand_home(project);
        if i > 0 {
            println!();
        }
        println!("{} ({})", detect::project_name(&path), tui::tilde(&path));
        print_processes(&path);
    }
    Ok(())
}

fn print_processes(path: &Path) {
    let specs = detect::detect(path);
    if specs.is_empty() {
        println!("  nothing to run found; add a paddock.toml to say what to run");
    }
    for spec in specs {
        let port = spec.port.map(|p| format!(":{p}")).unwrap_or_default();
        println!("  {:<14} {:<7} {}", spec.name, port, spec.command);
    }
}
