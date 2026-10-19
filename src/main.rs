mod core;
mod daemon;
mod loaders;
mod modrinth;
mod cli;

use clap::Parser;
use colored::Colorize;
use cli::args::{Cli, Commands};
use core::paths::SystemPaths;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let paths = SystemPaths::resolve();
    paths.ensure_dirs()?;

    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Create { name, loader, mc_version, ram, port }) => {
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
        Some(Commands::Logs { name, follow, lines }) => {
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
            // Si se ejecuta 'mcserman' sin argumentos y está en una terminal interactiva (TTY)
            if cli::prompts::is_tty() {
                println!("{}", "⛏️  mcserman - Gestor de Servidores de Minecraft".cyan().bold());
                let options = vec![
                    "1. Listar servidores existentes",
                    "2. Iniciar un servidor",
                    "3. Ver logs en vivo (tail -f)",
                    "4. Abrir consola interactiva",
                    "5. Detener un servidor",
                    "6. Crear nuevo servidor",
                    "7. Instalar mod o plugin desde Modrinth",
                    "8. Salir",
                ];

                let selection = cli::prompts::handle_inquire_cancel(
                    inquire::Select::new("¿Qué deseas hacer?", options).prompt()
                )?;

                if selection.starts_with("1") {
                    cli::handle_list(&paths).await?;
                } else if selection.starts_with("2") {
                    cli::handle_start(None, &paths).await?;
                } else if selection.starts_with("3") {
                    cli::handle_logs(None, true, 50, &paths).await?;
                } else if selection.starts_with("4") {
                    cli::handle_console(None, &paths).await?;
                } else if selection.starts_with("5") {
                    cli::handle_stop(None, &paths).await?;
                } else if selection.starts_with("6") {
                    cli::handle_create(None, None, None, None, None, &paths).await?;
                } else if selection.starts_with("7") {
                    cli::handle_add_mod(None, None, &paths).await?;
                }
            } else {
                // Ejecución desatendida sin argumentos: imprimir ayuda
                use clap::CommandFactory;
                let mut cmd = Cli::command();
                cmd.print_help()?;
                println!();
            }
        }
    }

    Ok(())
}
