//! The `paddock` binary. Installs error reporting and hands off to
//! `paddock::cli`. Keeps no logic of its own.
//!
//! File logging (tracing) arrives with the supervisor, when there is
//! something worth logging.

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;
    paddock::cli::run().await
}
