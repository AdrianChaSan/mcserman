use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "mcserman")]
#[command(author = "AdrianChaSan")]
#[command(version = "0.1.0")]
#[command(about = "A high-performance, host-independent Minecraft Dedicated Server Manager in Rust", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Crear una nueva instancia de servidor de Minecraft
    Create {
        /// Nombre identificador de la instancia
        name: Option<String>,

        /// Tipo de modloader (vanilla, paper, fabric)
        #[arg(short, long)]
        loader: Option<String>,

        /// Versión de Minecraft (ej. 1.21.4)
        #[arg(short, long)]
        mc_version: Option<String>,

        /// Memoria RAM máxima asignada (ej. 4G, 8G)
        #[arg(short, long)]
        ram: Option<String>,

        /// Puerto TCP para el servidor (por defecto 25565)
        #[arg(short, long)]
        port: Option<u16>,
    },

    /// Listar todas las instancias existentes y su estado
    List,

    /// Iniciar una instancia del servidor
    Start {
        /// Nombre de la instancia a iniciar
        name: Option<String>,
    },

    /// Detener una instancia limpiamente con 'stop'
    Stop {
        /// Nombre de la instancia a detener
        name: Option<String>,
    },

    /// Reiniciar una instancia
    Restart {
        /// Nombre de la instancia a reiniciar
        name: Option<String>,
    },

    /// Mostrar el estado de ejecución de una instancia
    Status {
        /// Nombre de la instancia
        name: Option<String>,
    },

    /// Ver los logs del servidor en tiempo real (equivalente a tail -f)
    Logs {
        /// Nombre de la instancia
        name: Option<String>,

        /// Seguir la salida de logs en vivo (-f / --follow)
        #[arg(short, long, default_value_t = false)]
        follow: bool,

        /// Número de líneas iniciales a mostrar
        #[arg(short, long, default_value_t = 50)]
        lines: usize,
    },

    /// Enviar un comando a la consola de una instancia en ejecución
    Cmd {
        /// Nombre de la instancia
        name: String,

        /// Comando a ejecutar (ej. "op Steve", "time set day")
        command: String,
    },

    /// Entrar a la consola interactiva en vivo
    Console {
        /// Nombre de la instancia
        name: Option<String>,
    },

    /// Buscar e instalar mods o plugins desde Modrinth
    AddMod {
        /// Nombre de la instancia
        name: Option<String>,

        /// Búsqueda del mod o plugin
        query: Option<String>,
    },

    /// Subcomando interno: ejecuta el supervisor del daemon para systemd o segundo plano
    #[command(hide = true)]
    Daemon {
        /// Nombre de la instancia
        name: String,
    },
}
