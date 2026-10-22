mod core;
mod daemon;
mod loaders;
mod modrinth;
mod tui;

use clap::{Parser, Subcommand};
use core::paths::SystemPaths;

#[derive(Parser, Debug)]
#[command(
    name = "mcserman",
    author = "AdrianChaSan",
    version,
    about = "A terminal Minecraft server manager built with Rust and ratatui",
    long_about = None
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Internal subcommand: supervisor daemon for background / systemd execution
    #[command(hide = true)]
    Daemon {
        /// Identifier name of the instance
        name: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let paths = SystemPaths::resolve();
    paths.ensure_dirs()?;

    match cli.command {
        Some(Commands::Daemon { name }) => {
            daemon::supervisor::run_daemon(&name, &paths).await?;
        }
        None => {
            tui::run(&paths).await?;
        }
    }

    Ok(())
}
