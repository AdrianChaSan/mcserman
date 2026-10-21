use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::collections::VecDeque;
use std::time::Instant;
use tui_input::backend::crossterm::EventHandler;
use tui_input::Input;

use crate::core::config::{InstanceConfig, LoaderType};
use crate::core::paths::SystemPaths;
use crate::core::properties::ServerProperties;
use crate::daemon::socket::{is_daemon_running, send_request, SocketRequest, SocketResponse};
use crate::modrinth::api::SearchHit;
use crate::tui::action::Action;

pub const MAX_LOG_LINES: usize = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Insert,
}

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

    pub fn title(&self) -> &'static str {
        match self {
            AppTab::Dashboard => "1. Servidor",
            AppTab::Console => "2. Consola",
            AppTab::Modrinth => "3. Modrinth",
            AppTab::Settings => "4. Opciones",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerState {
    Online,
    Offline,
    Starting,
    Stopping,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Sidebar,
    MainPanel,
}

#[derive(Debug, Clone)]
pub struct InstanceSummary {
    pub name: String,
    pub loader: String,
    pub mc_version: String,
    pub ram: String,
    pub port: u16,
    pub state: ServerState,
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
                    // Secuencia CSI: \x1b[ ... byte final de comando (0x40..=0x7E)
                    chars.next(); // consumir '['
                    while let Some(&next_c) = chars.peek() {
                        chars.next();
                        let b = next_c as u32;
                        if (0x40..=0x7E).contains(&b) {
                            break;
                        }
                    }
                }
                Some(']') => {
                    // Secuencia OSC: \x1b] ... \x07 o \x1b\
                    chars.next(); // consumir ']'
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
                    chars.next(); // consumir '(' o ')'
                    chars.next(); // consumir byte charset
                }
                Some(_) => {
                    // Secuencia de escape de 2 bytes: \x1b seguido de 1 caracter
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

#[cfg(unix)]
pub fn is_pid_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}

#[cfg(not(unix))]
pub fn is_pid_alive(_pid: u32) -> bool {
    false
}

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

pub struct App {
    pub should_quit: bool,
    pub input_mode: InputMode,
    pub active_tab: AppTab,
    pub focus: Focus,
    pub instances: Vec<InstanceSummary>,
    pub selected_instance: usize,
    pub paths: SystemPaths,
    pub status_message: Option<(String, Instant)>,
    pub terminal_size: (u16, u16),

    // Consola
    pub log_buffer: VecDeque<String>,
    pub console_scroll: usize,
    pub auto_scroll: bool,
    pub console_input: Input,
    pub console_task: Option<tokio::task::JoinHandle<()>>,
    pub active_console_instance: Option<String>,
    pub log_tx: tokio::sync::mpsc::UnboundedSender<(String, String)>,
    pub log_rx: Option<tokio::sync::mpsc::UnboundedReceiver<(String, String)>>,

    // Modrinth
    pub modrinth_input: Input,
    pub modrinth_hits: Vec<SearchHit>,
    pub selected_mod_hit: usize,
    pub modrinth_loading: bool,
    pub modrinth_progress: Option<(f64, String)>,
    pub modrinth_error: Option<String>,

    // Propiedades
    pub properties_list: Vec<(String, String)>,
    pub selected_property: usize,
    pub properties_dirty: bool,
    pub editing_property: Option<Input>,

    // Modal de Creación
    pub show_create_modal: bool,
    pub create_field_idx: usize, // 0: Name, 1: Loader, 2: Version, 3: RAM, 4: Port
    pub create_name_input: Input,
    pub create_loader_idx: usize, // 0: Paper, 1: Fabric, 2: Vanilla
    pub create_versions: Vec<String>,
    pub create_version_idx: usize,
    pub create_loading_versions: bool,
    pub create_ram_input: Input,
    pub create_port_input: Input,
    pub create_progress: Option<(f64, String)>,
    pub create_error: Option<String>,
}

impl App {
    pub fn new(paths: SystemPaths) -> Self {
        let (log_tx, log_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = Self {
            should_quit: false,
            input_mode: InputMode::Normal,
            active_tab: AppTab::Dashboard,
            focus: Focus::Sidebar,
            instances: Vec::new(),
            selected_instance: 0,
            paths,
            status_message: None,
            terminal_size: (80, 24),

            // Consola
            log_buffer: VecDeque::with_capacity(MAX_LOG_LINES),
            console_scroll: 0,
            auto_scroll: true,
            console_input: Input::default(),
            console_task: None,
            active_console_instance: None,
            log_tx,
            log_rx: Some(log_rx),

            // Modrinth
            modrinth_input: Input::default(),
            modrinth_hits: Vec::new(),
            selected_mod_hit: 0,
            modrinth_loading: false,
            modrinth_progress: None,
            modrinth_error: None,

            // Propiedades
            properties_list: Vec::new(),
            selected_property: 0,
            properties_dirty: false,
            editing_property: None,

            // Modal de Creación
            show_create_modal: false,
            create_field_idx: 0,
            create_name_input: Input::default(),
            create_loader_idx: 0,
            create_versions: Vec::new(),
            create_version_idx: 0,
            create_loading_versions: false,
            create_ram_input: Input::new("4G".to_string()),
            create_port_input: Input::new("25565".to_string()),
            create_progress: None,
            create_error: None,
        };
        app.reload_instances();
        app.load_properties_for_selected();
        app
    }

    pub fn reload_instances(&mut self) {
        let mut list = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&self.paths.instances_dir) {
            for entry in entries.flatten() {
                if entry.path().is_dir() {
                    let cfg_path = entry.path().join("instance.toml");
                    if let Ok(cfg) = InstanceConfig::load(&cfg_path) {
                        let existing_state = self
                            .instances
                            .iter()
                            .find(|i| i.name == cfg.name)
                            .map(|i| i.state)
                            .unwrap_or(ServerState::Offline);

                        list.push(InstanceSummary {
                            name: cfg.name,
                            loader: cfg.loader.to_string(),
                            mc_version: cfg.mc_version,
                            ram: cfg.ram_max,
                            port: cfg.port,
                            state: existing_state,
                        });
                    }
                }
            }
        }
        list.sort_by(|a, b| a.name.cmp(&b.name));
        self.instances = list;
        if self.selected_instance >= self.instances.len() && !self.instances.is_empty() {
            self.selected_instance = self.instances.len() - 1;
        }
    }

    pub async fn refresh_online_status(&mut self) {
        for inst in &mut self.instances {
            let detected = query_instance_state(&self.paths, &inst.name).await;
            match inst.state {
                ServerState::Starting => {
                    if detected == ServerState::Online {
                        inst.state = ServerState::Online;
                    } else if detected == ServerState::Offline {
                        inst.state = ServerState::Offline;
                    }
                }
                ServerState::Stopping => {
                    if detected == ServerState::Offline {
                        inst.state = ServerState::Offline;
                    }
                }
                _ => {
                    inst.state = detected;
                }
            }
        }
    }

    pub fn console_viewport_dims(&self) -> (usize, usize) {
        let (w, h) = self.terminal_size;
        let main_width = (w as usize).saturating_sub(crate::tui::theme::SIDEBAR_WIDTH as usize);
        let inner_width = main_width.saturating_sub(2);
        let logs_height = (h as usize).saturating_sub(3 + 1 + 3 + 3);
        let viewport_height = logs_height.saturating_sub(2);
        (inner_width, viewport_height)
    }

    pub fn console_total_visual_lines(&self) -> usize {
        let (inner_width, _) = self.console_viewport_dims();
        if inner_width == 0 {
            return 0;
        }
        self.log_buffer
            .iter()
            .map(|raw| {
                let len = raw.chars().count();
                len.max(1).div_ceil(inner_width)
            })
            .sum()
    }

    pub fn console_max_scroll(&self) -> usize {
        let (inner_width, viewport_height) = self.console_viewport_dims();
        if inner_width == 0 || viewport_height == 0 {
            return 0;
        }
        let total = self.console_total_visual_lines();
        total.saturating_sub(viewport_height)
    }

    pub fn selected(&self) -> Option<&InstanceSummary> {
        self.instances.get(self.selected_instance)
    }

    pub fn set_status(&mut self, msg: &str) {
        self.status_message = Some((msg.to_string(), Instant::now()));
    }

    pub fn check_status_expiration(&mut self) {
        if let Some((_, instant)) = self.status_message {
            if instant.elapsed() > std::time::Duration::from_secs(3) {
                self.status_message = None;
            }
        }
    }

    pub fn clear_status(&mut self) {
        self.status_message = None;
    }

    pub fn push_log_line(&mut self, line: String) {
        let clean = sanitize_log_line(&line);
        let trimmed = clean.trim_end();
        if trimmed.is_empty() && !line.trim().is_empty() {
            return;
        }
        if self.log_buffer.len() >= MAX_LOG_LINES {
            self.log_buffer.pop_front();
        }
        self.log_buffer.push_back(trimmed.to_string());
        if self.auto_scroll {
            self.console_scroll = 0;
        }
    }

    pub fn abort_console_stream(&mut self) {
        if let Some(handle) = self.console_task.take() {
            handle.abort();
        }
    }

    pub fn handle_log_line(&mut self, instance: &str, line: String) {
        if let Some(inst) = self.instances.iter_mut().find(|i| i.name == instance) {
            if inst.state == ServerState::Offline {
                inst.state = ServerState::Starting;
            }
        }
        if let Some(selected) = self.selected() {
            if selected.name == instance {
                self.push_log_line(line);
            }
        }
    }

    pub fn start_console_stream(&mut self) {
        let selected_name = match self.selected() {
            Some(s) => s.name.clone(),
            None => {
                self.abort_console_stream();
                self.active_console_instance = None;
                self.log_buffer.clear();
                return;
            }
        };

        // Si ya está activo el stream para esta misma instancia, no hacer nada (evita re-leer disco o reiniciar)
        if self.active_console_instance.as_deref() == Some(&selected_name)
            && self.console_task.is_some()
        {
            return;
        }

        self.abort_console_stream();
        self.active_console_instance = Some(selected_name.clone());
        self.log_buffer.clear();
        self.console_scroll = 0;
        self.auto_scroll = true;

        let log_path = self.paths.logs_file(&selected_name);
        let tx = self.log_tx.clone();
        let name_clone = selected_name.clone();

        let task = tokio::spawn(async move {
            // Lectura inicial del archivo: seek al final del archivo si es grande (máx 500 líneas)
            if let Ok(mut file) = tokio::fs::File::open(&log_path).await {
                use tokio::io::{AsyncBufReadExt, AsyncSeekExt, BufReader};
                let file_len = file.metadata().await.map(|m| m.len()).unwrap_or(0);
                const CHUNK_SIZE: u64 = 128 * 1024; // 128 KB
                let seek_pos = file_len.saturating_sub(CHUNK_SIZE);

                if seek_pos > 0 {
                    let _ = file.seek(std::io::SeekFrom::Start(seek_pos)).await;
                }

                let mut reader = BufReader::new(file);
                let mut line = String::new();

                // Si se hizo seek a mitad de archivo, descartar la primera línea incompleta
                if seek_pos > 0 {
                    let _ = reader.read_line(&mut line).await;
                    line.clear();
                }

                let mut lines = Vec::new();
                while let Ok(n) = reader.read_line(&mut line).await {
                    if n == 0 {
                        break;
                    }
                    let clean = sanitize_log_line(&line);
                    let trimmed = clean.trim_end();
                    if !trimmed.is_empty() {
                        lines.push(trimmed.to_string());
                    }
                    line.clear();
                }

                // Conservar máximo 500 líneas del tail inicial
                let start = if lines.len() > 500 {
                    lines.len() - 500
                } else {
                    0
                };
                for l in &lines[start..] {
                    if tx.send((name_clone.clone(), l.clone())).is_err() {
                        return;
                    }
                }

                // Streaming en vivo
                let mut poll_file = match tokio::fs::File::open(&log_path).await {
                    Ok(f) => f,
                    Err(_) => return,
                };
                let _ = poll_file.seek(std::io::SeekFrom::End(0)).await;
                let mut line_buf = String::new();
                let mut reader = BufReader::new(poll_file);

                loop {
                    tokio::select! {
                        res = reader.read_line(&mut line_buf) => {
                            match res {
                                Ok(0) => {
                                    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                                }
                                Ok(_) => {
                                    let clean = sanitize_log_line(&line_buf);
                                    let trimmed = clean.trim_end();
                                    if !trimmed.is_empty()
                                        && tx.send((name_clone.clone(), trimmed.to_string())).is_err()
                                    {
                                        break;
                                    }
                                    line_buf.clear();
                                }
                                Err(_) => break,
                            }
                        }
                    }
                }
            }
        });

        self.console_task = Some(task);
    }

    pub fn load_properties_for_selected(&mut self) {
        self.properties_list.clear();
        self.properties_dirty = false;
        self.selected_property = 0;

        let name = match self.selected() {
            Some(i) => i.name.clone(),
            None => return,
        };
        let props_path = self.paths.instance_dir(&name).join("server.properties");
        let props = ServerProperties::load_or_default(&props_path);

        // Claves esenciales recomendadas
        let keys = [
            "motd",
            "server-port",
            "difficulty",
            "gamemode",
            "max-players",
            "online-mode",
            "pvp",
            "white-list",
            "allow-flight",
            "view-distance",
        ];

        for k in keys {
            let val = props.get(k).unwrap_or("").to_string();
            self.properties_list.push((k.to_string(), val));
        }
    }

    pub fn save_properties(&mut self) -> Result<(), String> {
        let name = match self.selected() {
            Some(i) => i.name.clone(),
            None => return Err("No hay instancia seleccionada".to_string()),
        };
        let props_path = self.paths.instance_dir(&name).join("server.properties");
        let mut props = ServerProperties::load_or_default(&props_path);
        for (k, v) in &self.properties_list {
            props.set(k, v);
        }
        props.save(&props_path).map_err(|e| e.to_string())?;
        self.properties_dirty = false;
        self.set_status("✓ server.properties guardado con éxito");
        Ok(())
    }
}

pub fn is_boolean_property(k: &str, v: &str) -> bool {
    v == "true"
        || v == "false"
        || matches!(
            k,
            "online-mode"
                | "pvp"
                | "white-list"
                | "allow-flight"
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

pub fn is_enum_property(k: &str) -> bool {
    matches!(k, "difficulty" | "gamemode")
}

impl App {
    pub fn toggle_selected_property(&mut self) {
        if let Some((k, v)) = self.properties_list.get_mut(self.selected_property) {
            if is_boolean_property(k, v) {
                *v = if v == "true" {
                    "false".to_string()
                } else {
                    "true".to_string()
                };
                self.properties_dirty = true;
            } else if k == "difficulty" {
                *v = match v.as_str() {
                    "peaceful" => "easy".to_string(),
                    "easy" => "normal".to_string(),
                    "normal" => "hard".to_string(),
                    _ => "peaceful".to_string(),
                };
                self.properties_dirty = true;
            } else if k == "gamemode" {
                *v = match v.as_str() {
                    "survival" => "creative".to_string(),
                    "creative" => "adventure".to_string(),
                    "adventure" => "spectator".to_string(),
                    _ => "survival".to_string(),
                };
                self.properties_dirty = true;
            }
        }
    }

    /// Manejo de teclas desacoplado con máquina de estados estricta (Rule 1).
    pub fn handle_key(&mut self, key: KeyEvent) -> Option<Action> {
        // Si el modal de creación está abierto
        if self.show_create_modal {
            if self.input_mode == InputMode::Insert {
                match (key.modifiers, key.code) {
                    (KeyModifiers::NONE, KeyCode::Esc) => {
                        self.input_mode = InputMode::Normal;
                        return None;
                    }
                    (KeyModifiers::NONE, KeyCode::Enter) => {
                        self.input_mode = InputMode::Normal;
                        return None;
                    }
                    (KeyModifiers::NONE, KeyCode::Tab) => {
                        self.create_field_idx = (self.create_field_idx + 1) % 5;
                        return None;
                    }
                    (KeyModifiers::SHIFT, KeyCode::BackTab) => {
                        self.create_field_idx = if self.create_field_idx == 0 {
                            4
                        } else {
                            self.create_field_idx - 1
                        };
                        return None;
                    }
                    _ => {
                        match self.create_field_idx {
                            0 => {
                                self.create_name_input
                                    .handle_event(&crossterm::event::Event::Key(key));
                            }
                            3 => {
                                self.create_ram_input
                                    .handle_event(&crossterm::event::Event::Key(key));
                            }
                            4 => {
                                self.create_port_input
                                    .handle_event(&crossterm::event::Event::Key(key));
                            }
                            _ => {}
                        }
                        return None;
                    }
                }
            } else {
                // Modo Normal dentro del modal
                match (key.modifiers, key.code) {
                    (KeyModifiers::NONE, KeyCode::Esc) => {
                        return Some(Action::CloseCreateModal);
                    }
                    (KeyModifiers::NONE, KeyCode::Tab) | (KeyModifiers::NONE, KeyCode::Down) => {
                        return Some(Action::NextWizardField);
                    }
                    (KeyModifiers::SHIFT, KeyCode::BackTab) | (KeyModifiers::NONE, KeyCode::Up) => {
                        return Some(Action::PrevWizardField);
                    }
                    (KeyModifiers::NONE, KeyCode::Enter) => {
                        return Some(Action::SubmitCreateInstance);
                    }
                    (KeyModifiers::NONE, KeyCode::Char('i')) => {
                        if matches!(self.create_field_idx, 0 | 3 | 4) {
                            self.input_mode = InputMode::Insert;
                        }
                        return None;
                    }
                    _ => {
                        if self.create_field_idx == 1 {
                            match key.code {
                                KeyCode::Left => {
                                    let prev = if self.create_loader_idx == 0 {
                                        2
                                    } else {
                                        self.create_loader_idx - 1
                                    };
                                    return Some(Action::SelectWizardLoader(prev));
                                }
                                KeyCode::Right => {
                                    let next = (self.create_loader_idx + 1) % 3;
                                    return Some(Action::SelectWizardLoader(next));
                                }
                                _ => {}
                            }
                        }
                        if self.create_field_idx == 2 {
                            match key.code {
                                KeyCode::Left => {
                                    if !self.create_versions.is_empty()
                                        && self.create_version_idx > 0
                                    {
                                        self.create_version_idx -= 1;
                                    }
                                    return None;
                                }
                                KeyCode::Right => {
                                    if !self.create_versions.is_empty()
                                        && self.create_version_idx + 1 < self.create_versions.len()
                                    {
                                        self.create_version_idx += 1;
                                    }
                                    return None;
                                }
                                _ => {}
                            }
                        }
                        if matches!(self.create_field_idx, 0 | 3 | 4) {
                            if let KeyCode::Char(_) = key.code {
                                self.input_mode = InputMode::Insert;
                                match self.create_field_idx {
                                    0 => {
                                        self.create_name_input
                                            .handle_event(&crossterm::event::Event::Key(key));
                                    }
                                    3 => {
                                        self.create_ram_input
                                            .handle_event(&crossterm::event::Event::Key(key));
                                    }
                                    4 => {
                                        self.create_port_input
                                            .handle_event(&crossterm::event::Event::Key(key));
                                    }
                                    _ => {}
                                }
                            }
                        }
                        return None;
                    }
                }
            }
        }

        // MODO INSERT: Escribiendo en consola, Modrinth o editando propiedad
        if self.input_mode == InputMode::Insert {
            match (key.modifiers, key.code) {
                (KeyModifiers::NONE, KeyCode::Esc) => {
                    if self.editing_property.is_some() {
                        return Some(Action::CancelEditProperty);
                    }
                    self.input_mode = InputMode::Normal;
                    None
                }
                (KeyModifiers::NONE, KeyCode::Enter) => match self.active_tab {
                    AppTab::Console => Some(Action::SubmitConsoleCommand),
                    AppTab::Modrinth => Some(Action::SubmitModrinthSearch),
                    AppTab::Settings => {
                        if self.editing_property.is_some() {
                            Some(Action::SubmitEditProperty)
                        } else {
                            None
                        }
                    }
                    _ => None,
                },
                (KeyModifiers::NONE, KeyCode::PageUp) => match self.active_tab {
                    AppTab::Console => Some(Action::ScrollLogUp(15)),
                    _ => None,
                },
                (KeyModifiers::NONE, KeyCode::PageDown) => match self.active_tab {
                    AppTab::Console => Some(Action::ScrollLogDown(15)),
                    _ => None,
                },
                _ => {
                    match self.active_tab {
                        AppTab::Console => {
                            self.console_input
                                .handle_event(&crossterm::event::Event::Key(key));
                        }
                        AppTab::Modrinth => {
                            self.modrinth_input
                                .handle_event(&crossterm::event::Event::Key(key));
                        }
                        AppTab::Settings => {
                            if let Some(ref mut input) = self.editing_property {
                                input.handle_event(&crossterm::event::Event::Key(key));
                            }
                        }
                        _ => {}
                    }
                    None
                }
            }
        } else {
            // MODO NORMAL: Navegación global activa
            match (key.modifiers, key.code) {
                (KeyModifiers::CONTROL, KeyCode::Char('c'))
                | (KeyModifiers::NONE, KeyCode::Char('q')) => Some(Action::Quit),
                (KeyModifiers::NONE, KeyCode::Char('h')) | (KeyModifiers::NONE, KeyCode::Left) => {
                    Some(Action::FocusSidebar)
                }
                (KeyModifiers::NONE, KeyCode::Char('l')) | (KeyModifiers::NONE, KeyCode::Right) => {
                    Some(Action::FocusMainPanel)
                }
                (KeyModifiers::NONE, KeyCode::Char('j')) | (KeyModifiers::NONE, KeyCode::Down) => {
                    Some(Action::Down)
                }
                (KeyModifiers::NONE, KeyCode::Char('k')) | (KeyModifiers::NONE, KeyCode::Up) => {
                    Some(Action::Up)
                }
                (KeyModifiers::NONE, KeyCode::Tab) => Some(Action::NextTab),
                (KeyModifiers::SHIFT, KeyCode::BackTab) => Some(Action::PrevTab),
                (KeyModifiers::NONE, KeyCode::Char('1')) => Some(Action::GoToTab(0)),
                (KeyModifiers::NONE, KeyCode::Char('2')) => Some(Action::GoToTab(1)),
                (KeyModifiers::NONE, KeyCode::Char('3')) => Some(Action::GoToTab(2)),
                (KeyModifiers::NONE, KeyCode::Char('4')) => Some(Action::GoToTab(3)),
                (KeyModifiers::NONE, KeyCode::Char('r')) => Some(Action::RefreshInstances),
                (KeyModifiers::NONE, KeyCode::Char('n')) => Some(Action::OpenCreateModal),
                (KeyModifiers::NONE, KeyCode::Char(' ')) => {
                    if self.focus == Focus::MainPanel && self.active_tab == AppTab::Settings {
                        Some(Action::ToggleProperty)
                    } else {
                        Some(Action::ToggleServer)
                    }
                }
                (KeyModifiers::NONE, KeyCode::Char('i')) => match self.active_tab {
                    AppTab::Console | AppTab::Modrinth => {
                        self.focus = Focus::MainPanel;
                        self.input_mode = InputMode::Insert;
                        None
                    }
                    AppTab::Settings => {
                        self.focus = Focus::MainPanel;
                        Some(Action::StartEditProperty)
                    }
                    _ => None,
                },
                (KeyModifiers::NONE, KeyCode::Enter) => {
                    if self.focus == Focus::MainPanel {
                        match self.active_tab {
                            AppTab::Console => {
                                self.input_mode = InputMode::Insert;
                                None
                            }
                            AppTab::Modrinth => Some(Action::InstallSelectedMod),
                            AppTab::Settings => {
                                if let Some((k, v)) =
                                    self.properties_list.get(self.selected_property)
                                {
                                    if is_boolean_property(k, v) || is_enum_property(k) {
                                        Some(Action::ToggleProperty)
                                    } else {
                                        Some(Action::StartEditProperty)
                                    }
                                } else {
                                    None
                                }
                            }
                            _ => None,
                        }
                    } else {
                        match self.active_tab {
                            AppTab::Console | AppTab::Modrinth => {
                                self.focus = Focus::MainPanel;
                                self.input_mode = InputMode::Insert;
                                None
                            }
                            _ => None,
                        }
                    }
                }
                (KeyModifiers::NONE, KeyCode::PageUp) => Some(Action::ScrollLogUp(15)),
                (KeyModifiers::NONE, KeyCode::PageDown) => Some(Action::ScrollLogDown(15)),
                (KeyModifiers::NONE, KeyCode::Home) => match self.active_tab {
                    AppTab::Console => Some(Action::ScrollLogUp(usize::MAX)),
                    _ => None,
                },
                (KeyModifiers::NONE, KeyCode::End) => match self.active_tab {
                    AppTab::Console => Some(Action::ScrollLogBottom),
                    _ => None,
                },
                (KeyModifiers::NONE, KeyCode::Char('s')) => {
                    if self.active_tab == AppTab::Settings {
                        Some(Action::SaveProperties)
                    } else {
                        None
                    }
                }
                _ => None,
            }
        }
    }

    pub fn update(
        &mut self,
        action: Action,
        event_tx: &tokio::sync::mpsc::Sender<crate::tui::event::AppEvent>,
    ) {
        match action {
            Action::Quit => {
                self.abort_console_stream();
                self.should_quit = true;
            }
            Action::FocusSidebar => {
                self.clear_status();
                self.focus = Focus::Sidebar;
            }
            Action::FocusMainPanel => {
                self.clear_status();
                self.focus = Focus::MainPanel;
            }
            Action::ToggleFocus => {
                self.clear_status();
                self.focus = match self.focus {
                    Focus::Sidebar => Focus::MainPanel,
                    Focus::MainPanel => Focus::Sidebar,
                };
            }
            Action::Up => {
                if self.focus == Focus::MainPanel {
                    if self.active_tab == AppTab::Console {
                        let max_scroll = self.console_max_scroll();
                        if max_scroll > 0 {
                            self.console_scroll =
                                self.console_scroll.saturating_add(1).min(max_scroll);
                            self.auto_scroll = false;
                        } else {
                            self.console_scroll = 0;
                        }
                        return;
                    }
                    if self.active_tab == AppTab::Settings && !self.properties_list.is_empty() {
                        if self.selected_property > 0 {
                            self.selected_property -= 1;
                        } else {
                            self.selected_property = self.properties_list.len() - 1;
                        }
                        return;
                    }
                    if self.active_tab == AppTab::Modrinth && !self.modrinth_hits.is_empty() {
                        if self.selected_mod_hit > 0 {
                            self.selected_mod_hit -= 1;
                        } else {
                            self.selected_mod_hit = self.modrinth_hits.len() - 1;
                        }
                        return;
                    }
                }
                if !self.instances.is_empty() {
                    let prev_idx = self.selected_instance;
                    if self.selected_instance == 0 {
                        self.selected_instance = self.instances.len() - 1;
                    } else {
                        self.selected_instance -= 1;
                    }
                    if prev_idx != self.selected_instance {
                        self.clear_status();
                        self.log_buffer.clear();
                        self.console_scroll = 0;
                        self.auto_scroll = true;
                        self.active_console_instance = None;
                        self.start_console_stream();
                        self.load_properties_for_selected();
                    }
                }
            }
            Action::Down => {
                if self.focus == Focus::MainPanel {
                    if self.active_tab == AppTab::Console {
                        let max_scroll = self.console_max_scroll();
                        self.console_scroll =
                            self.console_scroll.min(max_scroll).saturating_sub(1);
                        if self.console_scroll == 0 {
                            self.auto_scroll = true;
                        }
                        return;
                    }
                    if self.active_tab == AppTab::Settings && !self.properties_list.is_empty() {
                        if self.selected_property + 1 < self.properties_list.len() {
                            self.selected_property += 1;
                        } else {
                            self.selected_property = 0;
                        }
                        return;
                    }
                    if self.active_tab == AppTab::Modrinth && !self.modrinth_hits.is_empty() {
                        if self.selected_mod_hit + 1 < self.modrinth_hits.len() {
                            self.selected_mod_hit += 1;
                        } else {
                            self.selected_mod_hit = 0;
                        }
                        return;
                    }
                }
                if !self.instances.is_empty() {
                    let prev_idx = self.selected_instance;
                    if self.selected_instance + 1 >= self.instances.len() {
                        self.selected_instance = 0;
                    } else {
                        self.selected_instance += 1;
                    }
                    if prev_idx != self.selected_instance {
                        self.clear_status();
                        self.log_buffer.clear();
                        self.console_scroll = 0;
                        self.auto_scroll = true;
                        self.active_console_instance = None;
                        self.start_console_stream();
                        self.load_properties_for_selected();
                    }
                }
            }
            Action::NextTab => {
                self.clear_status();
                let idx = self.active_tab as usize;
                let next = (idx + 1) % AppTab::ALL.len();
                self.active_tab = AppTab::ALL[next];
                self.focus = match self.active_tab {
                    AppTab::Dashboard | AppTab::Console => Focus::Sidebar,
                    AppTab::Modrinth | AppTab::Settings => Focus::MainPanel,
                };
                if self.active_tab == AppTab::Console {
                    self.start_console_stream();
                }
            }
            Action::PrevTab => {
                self.clear_status();
                let idx = self.active_tab as usize;
                let prev = if idx == 0 {
                    AppTab::ALL.len() - 1
                } else {
                    idx - 1
                };
                self.active_tab = AppTab::ALL[prev];
                self.focus = match self.active_tab {
                    AppTab::Dashboard | AppTab::Console => Focus::Sidebar,
                    AppTab::Modrinth | AppTab::Settings => Focus::MainPanel,
                };
                if self.active_tab == AppTab::Console {
                    self.start_console_stream();
                }
            }
            Action::GoToTab(idx) => {
                self.clear_status();
                if idx < AppTab::ALL.len() {
                    self.active_tab = AppTab::ALL[idx];
                    self.focus = match self.active_tab {
                        AppTab::Dashboard | AppTab::Console => Focus::Sidebar,
                        AppTab::Modrinth | AppTab::Settings => Focus::MainPanel,
                    };
                    if self.active_tab == AppTab::Console {
                        self.start_console_stream();
                    }
                }
            }
            Action::StartEditProperty => {
                if self.active_tab == AppTab::Settings && !self.properties_list.is_empty() {
                    let val = self.properties_list[self.selected_property].1.clone();
                    self.editing_property = Some(Input::new(val));
                    self.input_mode = InputMode::Insert;
                }
            }
            Action::CancelEditProperty => {
                self.editing_property = None;
                self.input_mode = InputMode::Normal;
            }
            Action::SubmitEditProperty => {
                if let Some(input) = self.editing_property.take() {
                    let new_val = input.value().to_string();
                    if let Some((_, v)) = self.properties_list.get_mut(self.selected_property) {
                        *v = new_val;
                        self.properties_dirty = true;
                    }
                }
                self.input_mode = InputMode::Normal;
            }
            Action::RefreshInstances => {
                self.reload_instances();
                self.load_properties_for_selected();
                self.set_status("Lista de servidores recargada");
            }
            Action::ToggleServer => {
                self.execute_toggle_server(event_tx);
            }
            Action::SubmitConsoleCommand => {
                let cmd = self.console_input.value().trim().to_string();
                if !cmd.is_empty() {
                    self.console_input.reset();
                    self.send_console_command(&cmd, event_tx);
                }
            }
            Action::ScrollLogUp(lines) => {
                let max_scroll = self.console_max_scroll();
                if max_scroll > 0 {
                    self.console_scroll = self.console_scroll.saturating_add(lines).min(max_scroll);
                    self.auto_scroll = false;
                } else {
                    self.console_scroll = 0;
                }
            }
            Action::ScrollLogDown(lines) => {
                let max_scroll = self.console_max_scroll();
                self.console_scroll = self.console_scroll.min(max_scroll).saturating_sub(lines);
                if self.console_scroll == 0 {
                    self.auto_scroll = true;
                }
            }
            Action::ScrollLogBottom => {
                self.console_scroll = 0;
                self.auto_scroll = true;
            }
            Action::SubmitModrinthSearch => {
                self.execute_modrinth_search(event_tx);
            }
            Action::InstallSelectedMod => {
                self.execute_install_mod(event_tx);
            }
            Action::ToggleProperty => {
                self.toggle_selected_property();
            }
            Action::SaveProperties => {
                let _ = self.save_properties();
            }
            Action::OpenCreateModal => {
                self.show_create_modal = true;
                self.input_mode = InputMode::Insert;
                self.create_field_idx = 0;
                self.create_error = None;
                self.create_progress = None;
                self.load_wizard_versions(event_tx);
            }
            Action::CloseCreateModal => {
                self.show_create_modal = false;
                self.input_mode = InputMode::Normal;
                self.create_error = None;
                self.create_progress = None;
            }
            Action::NextWizardField => {
                self.create_field_idx = (self.create_field_idx + 1) % 5;
            }
            Action::PrevWizardField => {
                self.create_field_idx = if self.create_field_idx == 0 {
                    4
                } else {
                    self.create_field_idx - 1
                };
            }
            Action::SelectWizardLoader(idx) => {
                if idx < 3 {
                    self.create_loader_idx = idx;
                    self.create_version_idx = 0;
                    self.load_wizard_versions(event_tx);
                }
            }
            Action::SubmitCreateInstance => {
                self.execute_create_instance(event_tx);
            }
            Action::Resize(w, h) => {
                self.terminal_size = (w, h);
            }
            Action::SetStatus(msg) => {
                self.set_status(&msg);
            }
            _ => {}
        }
    }

    pub fn execute_toggle_server(
        &mut self,
        event_tx: &tokio::sync::mpsc::Sender<crate::tui::event::AppEvent>,
    ) {
        let (inst_name, current_state) = match self.selected() {
            Some(i) => (i.name.clone(), i.state),
            None => return,
        };

        // Actualización reactiva inmediata en memoria
        if let Some(inst) = self.instances.get_mut(self.selected_instance) {
            match current_state {
                ServerState::Offline => {
                    inst.state = ServerState::Starting;
                    self.set_status(&format!("Iniciando '{}'...", inst_name));
                }
                ServerState::Online | ServerState::Starting => {
                    inst.state = ServerState::Stopping;
                    self.set_status(&format!("Deteniendo '{}'...", inst_name));
                }
                ServerState::Stopping => {}
            }
        }

        let paths = self.paths.clone();
        let tx = event_tx.clone();
        let name = inst_name.clone();

        tokio::spawn(async move {
            let sock = paths.socket_path(&name);
            let state = query_instance_state(&paths, &name).await;
            if state == ServerState::Online || is_daemon_running(&sock).await {
                let _ = tx
                    .send(crate::tui::event::AppEvent::InstanceStatusChanged {
                        name: name.clone(),
                        state: ServerState::Stopping,
                    })
                    .await;
                let _ = tx
                    .send(crate::tui::event::AppEvent::StatusMessage(format!(
                        "Deteniendo '{}'...",
                        name
                    )))
                    .await;
                let _ = crate::daemon::socket::send_request(
                    &sock,
                    &crate::daemon::socket::SocketRequest::Stop,
                )
                .await;
                for _ in 0..40 {
                    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                    let cur = query_instance_state(&paths, &name).await;
                    if cur == ServerState::Offline {
                        let _ = tx
                            .send(crate::tui::event::AppEvent::InstanceStatusChanged {
                                name: name.clone(),
                                state: ServerState::Offline,
                            })
                            .await;
                        let _ = tx
                            .send(crate::tui::event::AppEvent::StatusMessage(format!(
                                "✓ Servidor '{}' detenido.",
                                name
                            )))
                            .await;
                        break;
                    }
                }
            } else {
                let _ = tx
                    .send(crate::tui::event::AppEvent::InstanceStatusChanged {
                        name: name.clone(),
                        state: ServerState::Starting,
                    })
                    .await;
                let _ = tx
                    .send(crate::tui::event::AppEvent::StatusMessage(format!(
                        "Iniciando '{}'...",
                        name
                    )))
                    .await;
                match crate::daemon::supervisor::spawn_daemon_detached(&name) {
                    Ok(()) => {
                        for _ in 0..60 {
                            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                            let cur = query_instance_state(&paths, &name).await;
                            if cur == ServerState::Online {
                                let _ = tx
                                    .send(crate::tui::event::AppEvent::InstanceStatusChanged {
                                        name: name.clone(),
                                        state: ServerState::Online,
                                    })
                                    .await;
                                let _ = tx
                                    .send(crate::tui::event::AppEvent::StatusMessage(format!(
                                        "✓ Servidor '{}' ONLINE.",
                                        name
                                    )))
                                    .await;
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        let _ = tx
                            .send(crate::tui::event::AppEvent::InstanceStatusChanged {
                                name: name.clone(),
                                state: ServerState::Offline,
                            })
                            .await;
                        let _ = tx
                            .send(crate::tui::event::AppEvent::StatusMessage(format!(
                                "Error al lanzar daemon: {}",
                                e
                            )))
                            .await;
                    }
                }
            }
        });
    }

    fn send_console_command(
        &self,
        cmd: &str,
        event_tx: &tokio::sync::mpsc::Sender<crate::tui::event::AppEvent>,
    ) {
        let inst = match self.selected() {
            Some(i) => i.clone(),
            None => return,
        };
        let paths = self.paths.clone();
        let cmd = cmd.to_string();
        let tx = event_tx.clone();

        tokio::spawn(async move {
            let sock = paths.socket_path(&inst.name);
            if is_daemon_running(&sock).await {
                match crate::daemon::socket::send_request(
                    &sock,
                    &crate::daemon::socket::SocketRequest::Command(cmd.clone()),
                )
                .await
                {
                    Ok(crate::daemon::socket::SocketResponse::Ok(msg)) => {
                        let _ = tx
                            .send(crate::tui::event::AppEvent::StatusMessage(msg))
                            .await;
                    }
                    Ok(crate::daemon::socket::SocketResponse::Error(err)) => {
                        let _ = tx
                            .send(crate::tui::event::AppEvent::StatusMessage(format!(
                                "Error: {}",
                                err
                            )))
                            .await;
                    }
                    Err(e) => {
                        let _ = tx
                            .send(crate::tui::event::AppEvent::StatusMessage(format!(
                                "Fallo socket: {}",
                                e
                            )))
                            .await;
                    }
                }
            } else {
                let _ = tx
                    .send(crate::tui::event::AppEvent::StatusMessage(format!(
                        "El servidor '{}' está apagado",
                        inst.name
                    )))
                    .await;
            }
        });
    }

    fn execute_modrinth_search(
        &mut self,
        event_tx: &tokio::sync::mpsc::Sender<crate::tui::event::AppEvent>,
    ) {
        let query = self.modrinth_input.value().trim().to_string();
        if query.is_empty() {
            return;
        }
        let inst = match self.selected() {
            Some(i) => i.clone(),
            None => return,
        };

        if inst.loader == "vanilla" {
            self.modrinth_error =
                Some("Vanilla no soporta mods o plugins de Modrinth directamente.".to_string());
            return;
        }

        self.modrinth_loading = true;
        self.modrinth_error = None;
        let tx = event_tx.clone();

        tokio::spawn(async move {
            let client = crate::modrinth::api::ModrinthClient::new();
            match client.search(&query, &inst.loader, &inst.mc_version).await {
                Ok(hits) => {
                    let _ = tx
                        .send(crate::tui::event::AppEvent::ModrinthSearchResults(hits))
                        .await;
                }
                Err(e) => {
                    let _ = tx
                        .send(crate::tui::event::AppEvent::ModrinthSearchError(
                            e.to_string(),
                        ))
                        .await;
                }
            }
        });
    }

    fn execute_install_mod(
        &mut self,
        event_tx: &tokio::sync::mpsc::Sender<crate::tui::event::AppEvent>,
    ) {
        let hit = match self.modrinth_hits.get(self.selected_mod_hit) {
            Some(h) => h.clone(),
            None => return,
        };
        let inst = match self.selected() {
            Some(i) => i.clone(),
            None => return,
        };
        let paths = self.paths.clone();
        let tx = event_tx.clone();
        self.modrinth_progress = Some((0.0, format!("Descargando {}...", hit.title)));

        tokio::spawn(async move {
            let client = crate::modrinth::api::ModrinthClient::new();
            match client
                .get_compatible_version(&hit.project_id, &inst.loader, &inst.mc_version)
                .await
            {
                Ok(ver) => {
                    let file = ver
                        .files
                        .iter()
                        .find(|f| f.primary)
                        .or_else(|| ver.files.first());
                    if let Some(file_info) = file {
                        let target_dir = if inst.loader == "paper" {
                            paths.instance_dir(&inst.name).join("plugins")
                        } else {
                            paths.instance_dir(&inst.name).join("mods")
                        };

                        let (prog_tx, mut prog_rx) = tokio::sync::mpsc::channel(64);
                        let reporter = crate::core::progress::ChannelProgressReporter::new(prog_tx);

                        let tx_clone = tx.clone();
                        let progress_task = tokio::spawn(async move {
                            while let Some(up) = prog_rx.recv().await {
                                let _ = tx_clone
                                    .send(crate::tui::event::AppEvent::Progress(up))
                                    .await;
                            }
                        });

                        match client
                            .download_file(file_info, &target_dir, &reporter)
                            .await
                        {
                            Ok(_) => {
                                let _ = tx
                                    .send(crate::tui::event::AppEvent::ModrinthInstalled(
                                        hit.title.clone(),
                                    ))
                                    .await;
                            }
                            Err(e) => {
                                let _ = tx
                                    .send(crate::tui::event::AppEvent::ModrinthSearchError(
                                        e.to_string(),
                                    ))
                                    .await;
                            }
                        }
                        progress_task.abort();
                    } else {
                        let _ = tx
                            .send(crate::tui::event::AppEvent::ModrinthSearchError(
                                "No contiene archivos descargables".to_string(),
                            ))
                            .await;
                    }
                }
                Err(e) => {
                    let _ = tx
                        .send(crate::tui::event::AppEvent::ModrinthSearchError(
                            e.to_string(),
                        ))
                        .await;
                }
            }
        });
    }

    pub fn load_wizard_versions(
        &mut self,
        event_tx: &tokio::sync::mpsc::Sender<crate::tui::event::AppEvent>,
    ) {
        let loader = match self.create_loader_idx {
            0 => LoaderType::Paper,
            1 => LoaderType::Fabric,
            _ => LoaderType::Vanilla,
        };
        self.create_loading_versions = true;
        self.create_versions.clear();
        let tx = event_tx.clone();

        tokio::spawn(async move {
            match crate::loaders::fetch_available_versions(&loader).await {
                Ok(vers) => {
                    let _ = tx
                        .send(crate::tui::event::AppEvent::VersionsLoaded(vers))
                        .await;
                }
                Err(e) => {
                    let _ = tx
                        .send(crate::tui::event::AppEvent::VersionsLoadError(
                            e.to_string(),
                        ))
                        .await;
                }
            }
        });
    }

    fn execute_create_instance(
        &mut self,
        event_tx: &tokio::sync::mpsc::Sender<crate::tui::event::AppEvent>,
    ) {
        let name = self.create_name_input.value().trim().to_string();
        if name.is_empty() {
            self.create_error = Some("El nombre no puede estar vacío".to_string());
            return;
        }
        if !name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
        {
            self.create_error =
                Some("El nombre solo puede contener letras, números, '-' o '_'".to_string());
            return;
        }
        let inst_dir = self.paths.instance_dir(&name);
        if inst_dir.exists() {
            self.create_error = Some(format!("Ya existe una instancia con el nombre '{}'", name));
            return;
        }

        let loader = match self.create_loader_idx {
            0 => LoaderType::Paper,
            1 => LoaderType::Fabric,
            _ => LoaderType::Vanilla,
        };

        let version = match self.create_versions.get(self.create_version_idx) {
            Some(v) => v.clone(),
            None => "1.21.4".to_string(),
        };

        let ram = self.create_ram_input.value().trim().to_string();
        let port: u16 = match self.create_port_input.value().trim().parse() {
            Ok(p) if p > 1024 => p,
            _ => {
                self.create_error =
                    Some("Puerto TCP inválido (debe ser un número > 1024)".to_string());
                return;
            }
        };

        self.create_error = None;
        self.create_progress = Some((
            0.0,
            format!("Descargando servidor {} ({})...", loader, version),
        ));

        let paths = self.paths.clone();
        let tx = event_tx.clone();

        tokio::spawn(async move {
            let res = async {
                std::fs::create_dir_all(&inst_dir)?;
                let mut config = InstanceConfig::new(name.clone(), loader, version.clone());
                config.ram_max = ram.clone();
                config.port = port;

                let (prog_tx, mut prog_rx) = tokio::sync::mpsc::channel(64);
                let reporter = crate::core::progress::ChannelProgressReporter::new(prog_tx);

                let tx_clone = tx.clone();
                let progress_forwarder = tokio::spawn(async move {
                    while let Some(up) = prog_rx.recv().await {
                        let _ = tx_clone
                            .send(crate::tui::event::AppEvent::Progress(up))
                            .await;
                    }
                });

                let target_jar = inst_dir.join(&config.server_jar);
                let loader_ver =
                    crate::loaders::download_server_jar(loader, &version, &target_jar, &reporter)
                        .await?;
                config.loader_version = loader_ver;

                // Resolver Java respetando entorno NixOS
                let java_res =
                    crate::core::java::resolve_java(config.java_version, &paths, &reporter).await;
                if let Err(e) = java_res {
                    progress_forwarder.abort();
                    let _ = std::fs::remove_dir_all(&inst_dir);
                    return Err(e);
                }

                let props_path = inst_dir.join("server.properties");
                let mut props = ServerProperties::load_or_default(&props_path);
                props.set("server-port", &port.to_string());
                props.set(
                    "motd",
                    &format!("Servidor {} ({} {})", name, loader, version),
                );
                props.save(&props_path)?;

                std::fs::write(inst_dir.join("eula.txt"), "eula=true\n")?;
                config.save(&paths.instance_config(&name))?;

                progress_forwarder.abort();
                Ok::<String, anyhow::Error>(name)
            }
            .await;

            match res {
                Ok(n) => {
                    let _ = tx
                        .send(crate::tui::event::AppEvent::InstanceCreated(Ok(n)))
                        .await;
                }
                Err(e) => {
                    let _ = tx
                        .send(crate::tui::event::AppEvent::InstanceCreated(Err(
                            e.to_string()
                        )))
                        .await;
                }
            }
        });
    }
}
