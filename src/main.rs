//! The `paddock` binary. Installs error reporting and hands off to
//! `paddock::cli`. Keeps no logic of its own.
//!
//! Paddock has no log file: everything it does is visible in the TUI.

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;
    paddock::cli::run().await
}
