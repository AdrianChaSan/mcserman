use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::collections::VecDeque;
use std::time::Instant;
use tui_input::backend::crossterm::EventHandler;
use tui_input::Input;

use crate::core::config::{InstanceConfig, LoaderType};
use crate::core::paths::SystemPaths;
use crate::core::properties::ServerProperties;
use crate::daemon::socket::is_daemon_running;
use crate::modrinth::api::SearchHit;
use crate::tui::action::Action;

pub const MAX_LOG_LINES: usize = 1500;

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

#[derive(Debug, Clone)]
pub struct InstanceSummary {
    pub name: String,
    pub loader: String,
    pub mc_version: String,
    pub ram: String,
    pub port: u16,
    pub is_online: bool,
}

/// Sanitiza líneas crudas de consola eliminando secuencias ANSI de control,
/// convirtiendo tabs en espacios y quitando retornos de carro.
pub fn sanitize_log_line(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut in_escape = false;
    for c in raw.chars() {
        if c == '\x1b' {
            in_escape = true;
        } else if in_escape {
            if c.is_ascii_alphabetic() || c == 'm' || c == 'K' || c == 'H' || c == 'J' {
                in_escape = false;
            }
        } else if c == '\t' {
            out.push_str("    ");
        } else if c != '\r' {
            out.push(c);
        }
    }
    out
}

pub struct App {
    pub should_quit: bool,
    pub input_mode: InputMode,
    pub active_tab: AppTab,
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
        let mut app = Self {
            should_quit: false,
            input_mode: InputMode::Normal,
            active_tab: AppTab::Dashboard,
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
                        list.push(InstanceSummary {
                            name: cfg.name,
                            loader: cfg.loader.to_string(),
                            mc_version: cfg.mc_version,
                            ram: cfg.ram_max,
                            port: cfg.port,
                            is_online: false,
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
            let sock = self.paths.socket_path(&inst.name);
            inst.is_online = is_daemon_running(&sock).await;
        }
    }

    pub fn selected(&self) -> Option<&InstanceSummary> {
        self.instances.get(self.selected_instance)
    }

    pub fn set_status(&mut self, msg: &str) {
        self.status_message = Some((msg.to_string(), Instant::now()));
    }

    pub fn push_log_line(&mut self, line: String) {
        let clean = sanitize_log_line(&line);
        if self.log_buffer.len() >= MAX_LOG_LINES {
            self.log_buffer.pop_front();
        }
        self.log_buffer.push_back(clean);
        if self.auto_scroll {
            self.console_scroll = 0;
        }
    }

    pub fn abort_console_stream(&mut self) {
        if let Some(handle) = self.console_task.take() {
            handle.abort();
        }
    }

    pub fn start_console_stream(
        &mut self,
        event_tx: &tokio::sync::mpsc::Sender<crate::tui::event::AppEvent>,
    ) {
        self.abort_console_stream();
        let selected_name = match self.selected() {
            Some(s) => s.name.clone(),
            None => return,
        };
        let log_path = self.paths.logs_file(&selected_name);
        let tx = event_tx.clone();
        let name_clone = selected_name.clone();

        let task = tokio::spawn(async move {
            // Lectura inicial del archivo
            if let Ok(file) = tokio::fs::File::open(&log_path).await {
                use tokio::io::{AsyncBufReadExt, AsyncSeekExt, BufReader};
                let mut reader = BufReader::new(file);
                let mut lines = Vec::new();
                let mut line = String::new();
                while let Ok(n) = reader.read_line(&mut line).await {
                    if n == 0 {
                        break;
                    }
                    let trimmed = line.trim_end().to_string();
                    if !trimmed.is_empty() {
                        lines.push(trimmed);
                    }
                    line.clear();
                }

                // Enviar tail inicial (máximo 250 líneas)
                let start = if lines.len() > 250 {
                    lines.len() - 250
                } else {
                    0
                };
                for l in &lines[start..] {
                    let _ = tx
                        .send(crate::tui::event::AppEvent::LogLineReceived {
                            instance: name_clone.clone(),
                            line: l.clone(),
                        })
                        .await;
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
                                    let trimmed = line_buf.trim_end().to_string();
                                    if !trimmed.is_empty()
                                        && tx
                                            .send(crate::tui::event::AppEvent::LogLineReceived {
                                                instance: name_clone.clone(),
                                                line: trimmed,
                                            })
                                            .await
                                            .is_err()
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

    pub fn toggle_selected_property(&mut self) {
        if let Some((k, v)) = self.properties_list.get_mut(self.selected_property) {
            match k.as_str() {
                "online-mode" | "pvp" | "white-list" | "allow-flight" => {
                    *v = if v == "true" {
                        "false".to_string()
                    } else {
                        "true".to_string()
                    };
                    self.properties_dirty = true;
                }
                "difficulty" => {
                    *v = match v.as_str() {
                        "peaceful" => "easy".to_string(),
                        "easy" => "normal".to_string(),
                        "normal" => "hard".to_string(),
                        _ => "peaceful".to_string(),
                    };
                    self.properties_dirty = true;
                }
                "gamemode" => {
                    *v = match v.as_str() {
                        "survival" => "creative".to_string(),
                        "creative" => "adventure".to_string(),
                        "adventure" => "spectator".to_string(),
                        _ => "survival".to_string(),
                    };
                    self.properties_dirty = true;
                }
                _ => {}
            }
        }
    }

    /// Manejo de teclas desacoplado con máquina de estados estricta (Rule 1).
    pub fn handle_key(&mut self, key: KeyEvent) -> Option<Action> {
        // Si el modal de creación está abierto
        if self.show_create_modal {
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
                _ => {
                    // Si el foco está en el selector de Loader
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
                    // Si el foco está en el selector de Versión
                    if self.create_field_idx == 2 {
                        match key.code {
                            KeyCode::Left => {
                                if !self.create_versions.is_empty() && self.create_version_idx > 0 {
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

                    // Pasar al input activo correspondiente
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
        }

        // MODO INSERT (Rule 1): Escribiendo en consola o Modrinth. Atajos globales DESACTIVADOS.
        if self.input_mode == InputMode::Insert {
            match (key.modifiers, key.code) {
                (KeyModifiers::NONE, KeyCode::Esc) => {
                    self.input_mode = InputMode::Normal;
                    None
                }
                (KeyModifiers::NONE, KeyCode::Enter) => match self.active_tab {
                    AppTab::Console => Some(Action::SubmitConsoleCommand),
                    AppTab::Modrinth => Some(Action::SubmitModrinthSearch),
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
                        _ => {}
                    }
                    None
                }
            }
        } else {
            // MODO NORMAL (Rule 1): Navegación global activa
            match (key.modifiers, key.code) {
                (KeyModifiers::CONTROL, KeyCode::Char('c'))
                | (KeyModifiers::NONE, KeyCode::Char('q')) => Some(Action::Quit),
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
                (KeyModifiers::NONE, KeyCode::Char(' ')) => match self.active_tab {
                    AppTab::Settings => Some(Action::ToggleProperty),
                    _ => Some(Action::ToggleServer),
                },
                (KeyModifiers::NONE, KeyCode::Char('i')) => {
                    self.input_mode = InputMode::Insert;
                    None
                }
                (KeyModifiers::NONE, KeyCode::Enter) => match self.active_tab {
                    AppTab::Console | AppTab::Modrinth => {
                        self.input_mode = InputMode::Insert;
                        None
                    }
                    AppTab::Settings => Some(Action::ToggleProperty),
                    _ => None,
                },
                (KeyModifiers::NONE, KeyCode::PageUp) => Some(Action::ScrollLogUp(15)),
                (KeyModifiers::NONE, KeyCode::PageDown) => Some(Action::ScrollLogDown(15)),
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
            Action::Up => {
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
                if !self.instances.is_empty() {
                    let prev_idx = self.selected_instance;
                    if self.selected_instance == 0 {
                        self.selected_instance = self.instances.len() - 1;
                    } else {
                        self.selected_instance -= 1;
                    }
                    if prev_idx != self.selected_instance {
                        self.log_buffer.clear();
                        self.console_scroll = 0;
                        self.auto_scroll = true;
                        self.start_console_stream(event_tx);
                        self.load_properties_for_selected();
                    }
                }
            }
            Action::Down => {
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
                if !self.instances.is_empty() {
                    let prev_idx = self.selected_instance;
                    if self.selected_instance + 1 >= self.instances.len() {
                        self.selected_instance = 0;
                    } else {
                        self.selected_instance += 1;
                    }
                    if prev_idx != self.selected_instance {
                        self.log_buffer.clear();
                        self.console_scroll = 0;
                        self.auto_scroll = true;
                        self.start_console_stream(event_tx);
                        self.load_properties_for_selected();
                    }
                }
            }
            Action::NextTab => {
                let idx = self.active_tab as usize;
                let next = (idx + 1) % AppTab::ALL.len();
                self.active_tab = AppTab::ALL[next];
                if self.active_tab == AppTab::Console && self.console_task.is_none() {
                    self.start_console_stream(event_tx);
                }
            }
            Action::PrevTab => {
                let idx = self.active_tab as usize;
                let prev = if idx == 0 {
                    AppTab::ALL.len() - 1
                } else {
                    idx - 1
                };
                self.active_tab = AppTab::ALL[prev];
                if self.active_tab == AppTab::Console && self.console_task.is_none() {
                    self.start_console_stream(event_tx);
                }
            }
            Action::GoToTab(idx) => {
                if idx < AppTab::ALL.len() {
                    self.active_tab = AppTab::ALL[idx];
                    if self.active_tab == AppTab::Console && self.console_task.is_none() {
                        self.start_console_stream(event_tx);
                    }
                }
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
                self.console_scroll = self.console_scroll.saturating_add(lines);
                self.auto_scroll = false;
            }
            Action::ScrollLogDown(lines) => {
                self.console_scroll = self.console_scroll.saturating_sub(lines);
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

    fn execute_toggle_server(
        &self,
        event_tx: &tokio::sync::mpsc::Sender<crate::tui::event::AppEvent>,
    ) {
        let inst = match self.selected() {
            Some(i) => i.clone(),
            None => return,
        };
        let paths = self.paths.clone();
        let tx = event_tx.clone();

        tokio::spawn(async move {
            let sock = paths.socket_path(&inst.name);
            if is_daemon_running(&sock).await {
                let _ = tx
                    .send(crate::tui::event::AppEvent::StatusMessage(format!(
                        "Deteniendo '{}'...",
                        inst.name
                    )))
                    .await;
                let _ = crate::daemon::socket::send_request(
                    &sock,
                    &crate::daemon::socket::SocketRequest::Stop,
                )
                .await;
                for _ in 0..20 {
                    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                    if !is_daemon_running(&sock).await {
                        let _ = tx
                            .send(crate::tui::event::AppEvent::InstanceStatusChanged {
                                name: inst.name.clone(),
                                is_online: false,
                            })
                            .await;
                        let _ = tx
                            .send(crate::tui::event::AppEvent::StatusMessage(format!(
                                "✓ Servidor '{}' detenido.",
                                inst.name
                            )))
                            .await;
                        break;
                    }
                }
            } else {
                let _ = tx
                    .send(crate::tui::event::AppEvent::StatusMessage(format!(
                        "Iniciando '{}'...",
                        inst.name
                    )))
                    .await;
                match crate::daemon::supervisor::spawn_daemon_detached(&inst.name) {
                    Ok(()) => {
                        for _ in 0..24 {
                            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                            if is_daemon_running(&sock).await {
                                let _ = tx
                                    .send(crate::tui::event::AppEvent::InstanceStatusChanged {
                                        name: inst.name.clone(),
                                        is_online: true,
                                    })
                                    .await;
                                let _ = tx
                                    .send(crate::tui::event::AppEvent::StatusMessage(format!(
                                        "✓ Servidor '{}' ONLINE.",
                                        inst.name
                                    )))
                                    .await;
                                break;
                            }
                        }
                    }
                    Err(e) => {
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
