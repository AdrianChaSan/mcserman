use crate::core::config::LoaderType;
use crate::core::paths::SystemPaths;
use crate::daemon::socket::{is_daemon_running, send_request, SocketRequest, SocketResponse};

pub const MAX_LOG_LINES: usize = 1000;

/// Modalidad de entrada del usuario: navegación de widgets (Normal) o captura de texto (Insert).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Insert,
}

/// Pestañas principales de navegación en la interfaz de usuario.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppTab {
    Dashboard,
    Console,
    Modrinth,
    Settings,
}

impl AppTab {
    pub const ALL: [AppTab; 4] = [
        AppTab::Dashboard,
        AppTab::Console,
        AppTab::Modrinth,
        AppTab::Settings,
    ];

    /// Etiqueta visual para la barra de pestañas.
    pub fn title(&self) -> &'static str {
        match self {
            AppTab::Dashboard => "1. Servidor",
            AppTab::Console => "2. Consola",
            AppTab::Modrinth => "3. Mods",
            AppTab::Settings => "4. Opciones",
        }
    }
}

/// Metadatos básicos de un archivo de mod o plugin instalado en la instancia.
#[derive(Debug, Clone)]
pub struct InstalledMod {
    pub file_name: String,
    pub size_bytes: u64,
}

/// Estado operativo en tiempo real de un servidor de Minecraft administrado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerState {
    Online,
    Offline,
    Starting,
    Stopping,
}

/// Foco de navegación visual activo entre la barra lateral de instancias y el panel central.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Sidebar,
    MainPanel,
}

/// Resumen del estado y configuración de una instancia de servidor para la barra lateral.
#[derive(Debug, Clone)]
pub struct InstanceSummary {
    pub name: String,
    pub loader: String,
    pub mc_version: String,
    pub ram: String,
    pub port: u16,
    pub state: ServerState,
    pub source_pack: Option<String>,
    pub loader_version: Option<String>,
    pub modpack_info: Option<crate::core::config::ModpackInfo>,
}

impl InstanceSummary {
    /// Devuelve la subcarpeta correspondiente a complementos ("plugins" o "mods") según el loader.
    pub fn addons_subfolder(&self) -> &'static str {
        self.loader
            .parse::<LoaderType>()
            .map(|l| l.addons_subfolder())
            .unwrap_or_else(|_| {
                if self.loader.eq_ignore_ascii_case("paper")
                    || self.loader.eq_ignore_ascii_case("purpur")
                {
                    "plugins"
                } else {
                    "mods"
                }
            })
    }
}

/// Sanitiza líneas crudas de consola eliminando secuencias ANSI de control,
/// convirtiendo tabs en espacios y resolviendo reescrituras por retorno de carro (\r).
pub fn sanitize_log_line(raw: &str) -> String {
    let has_trailing_newline = raw.ends_with('\n');
    let mut s = raw;
    if let Some(stripped) = s.strip_suffix("\r\n") {
        s = stripped;
    } else if let Some(stripped) = s.strip_suffix('\n') {
        s = stripped;
    }
    s = s.trim_end_matches('\r');

    // Si la línea contiene \r en medio (reescrituras de JLine/Paper o barras de progreso),
    // la última parte no vacía representa el texto final sobreescrito en pantalla.
    let text_to_process = if s.contains('\r') {
        s.split('\r').rfind(|part| !part.is_empty()).unwrap_or("")
    } else {
        s
    };

    let mut out = String::with_capacity(text_to_process.len());
    let mut chars = text_to_process.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\x1b' {
            match chars.peek() {
                Some('[') => {
                    chars.next();
                    while let Some(&next_c) = chars.peek() {
                        chars.next();
                        let b = next_c as u32;
                        if (0x40..=0x7E).contains(&b) {
                            break;
                        }
                    }
                }
                Some(']') => {
                    chars.next();
                    while let Some(next_c) = chars.next() {
                        if next_c == '\x07' {
                            break;
                        }
                        if next_c == '\x1b' && chars.peek() == Some(&'\\') {
                            chars.next();
                            break;
                        }
                    }
                }
                Some('(') | Some(')') => {
                    chars.next();
                    chars.next();
                }
                Some(_) => {
                    chars.next();
                }
                None => {}
            }
        } else if c == '\t' {
            out.push_str("    ");
        } else if c != '\r' {
            out.push(c);
        }
    }

    if has_trailing_newline {
        out.push('\n');
    }

    out
}

/// Comprueba mediante señal de sondeo si un proceso POSIX continúa en ejecución activa.
#[cfg(unix)]
pub fn is_pid_alive(pid: u32) -> bool {
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}

/// Comprueba mediante señal de sondeo si un proceso continúa en ejecución activa (fallback no-UNIX).
#[cfg(not(unix))]
pub fn is_pid_alive(_pid: u32) -> bool {
    false
}

/// Consulta el estado actual del servidor mediante socket IPC o verificación de archivos PID huérfanos.
pub async fn query_instance_state(paths: &SystemPaths, name: &str) -> ServerState {
    let sock = paths.socket_path(name);
    let pid_file = paths.pid_file(name);

    if sock.exists() {
        if let Ok(SocketResponse::Ok(status)) = send_request(&sock, &SocketRequest::Status).await {
            if status.contains("Starting") {
                return ServerState::Starting;
            } else {
                return ServerState::Online;
            }
        }
        if is_daemon_running(&sock).await {
            return ServerState::Online;
        }
    }

    if pid_file.exists() {
        if let Ok(content) = std::fs::read_to_string(&pid_file) {
            let mut any_alive = false;
            for line in content.lines() {
                if let Ok(pid) = line.trim().parse::<u32>() {
                    if is_pid_alive(pid) {
                        any_alive = true;
                        break;
                    }
                }
            }
            if any_alive {
                return ServerState::Starting;
            } else {
                let _ = std::fs::remove_file(&pid_file);
                if sock.exists() {
                    let _ = std::fs::remove_file(&sock);
                }
            }
        }
    }

    ServerState::Offline
}

pub const COMMON_PROPERTY_KEYS: &[&str] = &[
    "motd",
    "server-port",
    "difficulty",
    "gamemode",
    "max-players",
    "online-mode",
    "white-list",
    "pvp",
    "enable-command-block",
    "view-distance",
    "simulation-distance",
    "level-name",
    "allow-nether",
    "hardcore",
    "level-seed",
    "spawn-protection",
];

/// Comprueba si una clave de server.properties admite alternancia booleana (true/false).
pub fn is_boolean_property(k: &str, v: &str) -> bool {
    v == "true"
        || v == "false"
        || matches!(
            k,
            "online-mode"
                | "pvp"
                | "white-list"
                | "allow-flight"
                | "allow-nether"
                | "enable-command-block"
                | "spawn-animals"
                | "spawn-monsters"
                | "spawn-npcs"
                | "generate-structures"
                | "hardcore"
                | "snooper-enabled"
                | "enable-rcon"
                | "enable-query"
                | "enforce-whitelist"
        )
}

/// Determina si una clave corresponde a un conjunto enumerable de valores (ej. difficulty, gamemode).
pub fn is_enum_property(k: &str) -> bool {
    matches!(k, "difficulty" | "gamemode")
}
