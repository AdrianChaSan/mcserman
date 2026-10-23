pub mod actions;
pub mod handlers;
pub mod tasks;
pub mod types;

pub use types::*;

use ratatui::widgets::ListState;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::time::Instant;
use tui_input::Input;

use crate::core::config::InstanceConfig;
use crate::core::paths::SystemPaths;
use crate::core::properties::ServerProperties;

/// Estado global y núcleo reactivo de la aplicación TUI.
pub struct App {
    pub should_quit: bool,
    pub input_mode: InputMode,
    pub active_tab: AppTab,
    pub focus: Focus,
    pub instances: Vec<InstanceSummary>,
    pub selected_instance: usize,
    pub instances_list_state: RefCell<ListState>,
    pub paths: SystemPaths,
    pub status_message: Option<(String, Instant)>,
    pub terminal_size: (u16, u16),

    pub log_buffer: VecDeque<String>,
    pub console_scroll: usize,
    pub auto_scroll: bool,
    pub console_input: Input,
    pub console_task: Option<tokio::task::JoinHandle<()>>,
    pub active_console_instance: Option<String>,
    pub log_tx: tokio::sync::mpsc::UnboundedSender<(String, String)>,
    pub log_rx: Option<tokio::sync::mpsc::UnboundedReceiver<(String, String)>>,

    pub modrinth_progress: Option<(f64, String)>,
    pub installed_mods: Vec<InstalledMod>,
    pub selected_installed_mod: usize,
    pub installed_mods_state: RefCell<ListState>,
    pub show_add_mod_modal: bool,
    pub add_mod_input: Input,
    pub add_mod_loading: bool,
    pub add_mod_error: Option<String>,
    pub show_delete_mod_modal: bool,
    pub delete_mod_filename: Option<String>,

    pub properties_list: Vec<(String, String)>,
    pub selected_property: usize,
    pub properties_list_state: RefCell<ListState>,
    pub properties_dirty: bool,
    pub editing_property: Option<Input>,
    pub editing_property_key: Option<String>,
    pub properties_search_input: Input,
    pub properties_search_active: bool,
    pub properties_show_all: bool,

    pub show_create_modal: bool,
    pub create_field_idx: usize, // 0: Name, 1: Loader, 2: Version/Modpack, 3: RAM, 4: Port
    pub create_name_input: Input,
    pub create_loader_idx: usize, // 0: Paper, 1: Fabric, 2: Vanilla, 3: Modpack
    pub create_versions: Vec<String>,
    pub create_version_idx: usize,
    pub create_loading_versions: bool,
    pub create_modpack_input: Input,
    pub create_modpack_resolving: bool,
    pub create_modpack_status: Option<String>,
    pub resolved_modpack: Option<crate::modrinth::modpack::ResolvedModpack>,
    pub create_ram_input: Input,
    pub create_port_input: Input,
    pub create_progress: Option<(f64, String)>,
    pub create_error: Option<String>,
    pub create_input_backup: Option<String>,

    pub show_delete_modal: bool,
    pub delete_server_name: Option<String>,
}

impl App {
    /// Inicializa la aplicación cargando instancias existentes, propiedades y configuraciones de terminal.
    pub fn new(paths: SystemPaths) -> Self {
        let (log_tx, log_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = Self {
            should_quit: false,
            input_mode: InputMode::Normal,
            active_tab: AppTab::Dashboard,
            focus: Focus::Sidebar,
            instances: Vec::new(),
            selected_instance: 0,
            instances_list_state: RefCell::new(ListState::default()),
            paths,
            status_message: None,
            terminal_size: (80, 24),

            log_buffer: VecDeque::with_capacity(MAX_LOG_LINES),
            console_scroll: 0,
            auto_scroll: true,
            console_input: Input::default(),
            console_task: None,
            active_console_instance: None,
            log_tx,
            log_rx: Some(log_rx),

            modrinth_progress: None,
            installed_mods: Vec::new(),
            selected_installed_mod: 0,
            installed_mods_state: RefCell::new(ListState::default()),
            show_add_mod_modal: false,
            add_mod_input: Input::default(),
            add_mod_loading: false,
            add_mod_error: None,
            show_delete_mod_modal: false,
            delete_mod_filename: None,

            properties_list: Vec::new(),
            selected_property: 0,
            properties_list_state: RefCell::new(ListState::default()),
            properties_dirty: false,
            editing_property: None,
            editing_property_key: None,
            properties_search_input: Input::default(),
            properties_search_active: false,
            properties_show_all: false,

            show_create_modal: false,
            create_field_idx: 0,
            create_name_input: Input::default(),
            create_loader_idx: 0,
            create_versions: Vec::new(),
            create_version_idx: 0,
            create_loading_versions: false,
            create_modpack_input: Input::default(),
            create_modpack_resolving: false,
            create_modpack_status: None,
            resolved_modpack: None,
            create_ram_input: Input::new("4G".to_string()),
            create_port_input: Input::new("25565".to_string()),
            create_progress: None,
            create_error: None,
            create_input_backup: None,

            show_delete_modal: false,
            delete_server_name: None,
        };
        app.reload_instances();
        app.load_properties_for_selected();
        app.load_installed_mods();
        app
    }

    /// Re-escanea el directorio de instancias actualizando el listado en memoria y sus estados.
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

                        let loader_version = crate::core::config::resolve_instance_loader_version(
                            &entry.path(),
                            cfg.loader_version.as_deref(),
                            cfg.loader,
                            &cfg.mc_version,
                        );

                        let modpack_info = crate::core::config::resolve_instance_modpack_info(
                            &entry.path(),
                            cfg.modpack_info.as_ref(),
                            cfg.source_pack.as_deref(),
                        );

                        list.push(InstanceSummary {
                            name: cfg.name,
                            loader: cfg.loader.to_string(),
                            mc_version: cfg.mc_version,
                            ram: cfg.ram_max,
                            port: cfg.port,
                            state: existing_state,
                            source_pack: cfg.source_pack,
                            loader_version,
                            modpack_info,
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

    /// Consulta asíncronamente el estado en tiempo real (online/offline) de todas las instancias registradas.
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

    /// Calcula el ancho y alto del área visible de logs descontando márgenes y bordes.
    pub fn console_viewport_dims(&self) -> (usize, usize) {
        let (w, h) = self.terminal_size;
        let main_width = (w as usize).saturating_sub(crate::tui::theme::SIDEBAR_WIDTH as usize);
        let inner_width = main_width.saturating_sub(2);
        let logs_height = (h as usize).saturating_sub(3 + 1 + 3 + 3);
        let viewport_height = logs_height.saturating_sub(2);
        (inner_width, viewport_height)
    }

    /// Calcula la cantidad total de líneas visuales ocupadas por el buffer aplicando ajuste de línea (wrap).
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

    /// Retorna el desplazamiento vertical máximo posible dentro del visor de logs.
    pub fn console_max_scroll(&self) -> usize {
        let (inner_width, viewport_height) = self.console_viewport_dims();
        if inner_width == 0 || viewport_height == 0 {
            return 0;
        }
        let total = self.console_total_visual_lines();
        total.saturating_sub(viewport_height)
    }

    /// Devuelve una referencia inmutable a la instancia actualmente seleccionada en la lista.
    pub fn selected(&self) -> Option<&InstanceSummary> {
        self.instances.get(self.selected_instance)
    }

    /// Comprueba si la instancia seleccionada fue originada o estructurada como un Server Pack.
    pub fn is_selected_serverpack(&self) -> bool {
        if let Some(inst) = self.selected() {
            if inst.source_pack.is_some() {
                return true;
            }
            let inst_dir = self.paths.instance_dir(&inst.name);
            if inst_dir.join("variables.txt").exists() {
                return true;
            }
        }
        false
    }

    /// Lee la carpeta de complementos (mods/plugins) de la instancia activa y carga la lista en memoria.
    pub fn load_installed_mods(&mut self) {
        let (inst_name, subfolder) = match self.selected() {
            Some(i) => (i.name.clone(), i.addons_subfolder()),
            None => {
                self.installed_mods.clear();
                self.selected_installed_mod = 0;
                *self.installed_mods_state.borrow_mut() = ListState::default();
                return;
            }
        };

        let dir = self.paths.instance_dir(&inst_name).join(subfolder);
        let mut list = Vec::new();
        if dir.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() {
                        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                            if name.ends_with(".jar") {
                                let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                                list.push(InstalledMod {
                                    file_name: name.to_string(),
                                    size_bytes: size,
                                });
                            }
                        }
                    }
                }
            }
        }
        list.sort_by_key(|a| a.file_name.to_lowercase());
        self.installed_mods = list;
        if self.installed_mods.is_empty() {
            self.selected_installed_mod = 0;
            *self.installed_mods_state.borrow_mut() = ListState::default();
        } else if self.selected_installed_mod >= self.installed_mods.len() {
            self.selected_installed_mod = self.installed_mods.len() - 1;
            self.installed_mods_state
                .borrow_mut()
                .select(Some(self.selected_installed_mod));
        }
    }

    /// Establece un mensaje de notificación efímero con marca de tiempo actual.
    pub fn set_status(&mut self, msg: &str) {
        self.status_message = Some((msg.to_string(), Instant::now()));
    }

    /// Limpia automáticamente el mensaje de estado si han transcurrido más de 3 segundos.
    pub fn check_status_expiration(&mut self) {
        if let Some((_, instant)) = self.status_message {
            if instant.elapsed() > std::time::Duration::from_secs(3) {
                self.status_message = None;
            }
        }
    }

    /// Borra de inmediato cualquier mensaje de estado activo.
    pub fn clear_status(&mut self) {
        self.status_message = None;
    }

    /// Inserta una línea sanitizada en el buffer circular de logs respetando la capacidad máxima.
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

    /// Cancela la tarea en segundo plano encargada de transmitir logs en vivo.
    pub fn abort_console_stream(&mut self) {
        if let Some(handle) = self.console_task.take() {
            handle.abort();
        }
    }

    /// Procesa una línea de log entrante asociándola a la instancia emisora y al buffer activo.
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

    /// Inicia la lectura y seguimiento en streaming del archivo de registros de la instancia seleccionada.
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
            let mut file_opt = None;
            for _ in 0..20 {
                if let Ok(f) = tokio::fs::File::open(&log_path).await {
                    file_opt = Some(f);
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(150)).await;
            }

            // Lectura inicial del archivo: seek al final del archivo si es grande (máx 500 líneas)
            if let Some(mut file) = file_opt {
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

    /// Carga las directivas de server.properties de la instancia seleccionada para el editor del Tab 4.
    pub fn load_properties_for_selected(&mut self) {
        self.properties_list.clear();
        self.properties_dirty = false;
        self.selected_property = 0;
        self.editing_property = None;
        self.editing_property_key = None;
        self.properties_search_input.reset();
        self.properties_search_active = false;
        self.properties_show_all = false;

        let name = match self.selected() {
            Some(i) => i.name.clone(),
            None => return,
        };
        let props_path = self.paths.instance_dir(&name).join("server.properties");
        let props = ServerProperties::load_or_default(&props_path);

        let mut seen = std::collections::HashSet::new();
        for &k in COMMON_PROPERTY_KEYS {
            let val = props.get(k).unwrap_or("").to_string();
            self.properties_list.push((k.to_string(), val));
            seen.insert(k);
        }
        for (k, v) in props.iter() {
            if !seen.contains(k.as_str()) {
                self.properties_list.push((k.clone(), v.clone()));
            }
        }
    }

    /// Filtra las propiedades de configuración según el modo activo (comunes vs todas) o consulta de búsqueda.
    pub fn filtered_properties(&self) -> Vec<(usize, &(String, String))> {
        let query = self.properties_search_input.value().trim().to_lowercase();
        if query.is_empty() {
            if self.properties_show_all {
                self.properties_list.iter().enumerate().collect()
            } else {
                let common_set: std::collections::HashSet<&str> =
                    COMMON_PROPERTY_KEYS.iter().copied().collect();
                self.properties_list
                    .iter()
                    .enumerate()
                    .filter(|(_, (k, _))| common_set.contains(k.as_str()))
                    .collect()
            }
        } else {
            self.properties_list
                .iter()
                .enumerate()
                .filter(|(_, (k, v))| {
                    k.to_lowercase().contains(&query) || v.to_lowercase().contains(&query)
                })
                .collect()
        }
    }

    /// Persiste los cambios de configuración pendientes al archivo server.properties en disco.
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

    /// Alterna el valor booleano o enum de la propiedad seleccionada en la lista.
    pub fn toggle_selected_property(&mut self) {
        let filtered = self.filtered_properties();
        let target_key = match filtered.get(self.selected_property) {
            Some(&(_, (k, _))) => k.clone(),
            None => return,
        };

        if let Some((k, v)) = self
            .properties_list
            .iter_mut()
            .find(|(k, _)| k == &target_key)
        {
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
}
