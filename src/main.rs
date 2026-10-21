mod cli;
mod core;
mod daemon;
mod loaders;
mod modrinth;
mod tui;

use clap::Parser;
use cli::args::{Cli, Commands};
use core::paths::SystemPaths;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let paths = SystemPaths::resolve();
    paths.ensure_dirs()?;

    match cli.command {
        Some(Commands::Create {
            name,
            loader,
            mc_version,
            ram,
            port,
        }) => {
            cli::handle_create(name, loader, mc_version, ram, port, &paths).await?;
        }
        Some(Commands::List) => {
            cli::handle_list(&paths).await?;
        }
        Some(Commands::Start { name }) => {
            cli::handle_start(name, &paths).await?;
        }
        Some(Commands::Stop { name }) => {
            cli::handle_stop(name, &paths).await?;
        }
        Some(Commands::Restart { name }) => {
            cli::handle_restart(name, &paths).await?;
        }
        Some(Commands::Status { name }) => {
            cli::handle_status(name, &paths).await?;
        }
        Some(Commands::Logs {
            name,
            follow,
            lines,
        }) => {
            cli::handle_logs(name, follow, lines, &paths).await?;
        }
        Some(Commands::Cmd { name, command }) => {
            cli::handle_cmd(&name, &command, &paths).await?;
        }
        Some(Commands::Console { name }) => {
            cli::handle_console(name, &paths).await?;
        }
        Some(Commands::AddMod { name, query }) => {
            cli::handle_add_mod(name, query, &paths).await?;
        }
        Some(Commands::Daemon { name }) => {
            daemon::supervisor::run_daemon(&name, &paths).await?;
        }
        None => {
            tui::run(&paths).await?;
        }
    }

    Ok(())
}
