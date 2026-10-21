use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "mcserman")]
#[command(author = "AdrianChaSan")]
#[command(version = "0.1.2")]
#[command(about = "A terminal Minecraft server manager built with Rust and ratatui", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Create a new Minecraft server instance
    Create {
        /// Identifier name of the instance
        name: Option<String>,

        /// Loader type (vanilla, paper, fabric)
        #[arg(short, long)]
        loader: Option<String>,

        /// Minecraft version (e.g. 1.21.4)
        #[arg(short, long)]
        mc_version: Option<String>,

        /// Maximum allocated RAM (e.g. 4G, 8G)
        #[arg(short, long)]
        ram: Option<String>,

        /// TCP port for the server (defaults to 25565)
        #[arg(short, long)]
        port: Option<u16>,
    },

    /// List all existing instances and their status
    List,

    /// Start a server instance
    Start {
        /// Name of the instance to start
        name: Option<String>,
    },

    /// Stop a server instance cleanly
    Stop {
        /// Name of the instance to stop
        name: Option<String>,
    },

    /// Restart a server instance
    Restart {
        /// Name of the instance to restart
        name: Option<String>,
    },

    /// Display the running status of an instance
    Status {
        /// Name of the instance
        name: Option<String>,
    },

    /// Stream server logs in real time
    Logs {
        /// Name of the instance
        name: Option<String>,

        /// Follow log output (-f / --follow)
        #[arg(short, long, default_value_t = false)]
        follow: bool,

        /// Number of initial lines to display
        #[arg(short, long, default_value_t = 50)]
        lines: usize,
    },

    /// Send a command to a running server instance
    Cmd {
        /// Name of the instance
        name: String,

        /// Command string to execute (e.g. "time set day")
        command: String,
    },

    /// Open an interactive console session
    Console {
        /// Name of the instance
        name: Option<String>,
    },

    /// Search and install mods or plugins from Modrinth
    AddMod {
        /// Name of the instance
        name: Option<String>,

        /// Search query
        query: Option<String>,
    },

    /// Internal subcommand: supervisor daemon for background / systemd execution
    #[command(hide = true)]
    Daemon {
        /// Name of the instance
        name: String,
    },
}
