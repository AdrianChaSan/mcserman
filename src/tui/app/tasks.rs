use super::{is_pid_alive, query_instance_state, App, AppTab, ServerState};
use crate::core::config::{InstanceConfig, LoaderType};
use crate::core::properties::ServerProperties;
use crate::daemon::socket::is_daemon_running;

impl App {
    /// Alterna el estado operativo (iniciar/detener) de la instancia seleccionada de forma asíncrona.
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

    /// Envía un comando RCON/stdin al servidor activo a través de su socket IPC.
    pub(crate) fn send_console_command(
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

    /// Descarga e instala un mod o plugin en segundo plano reportando el progreso a la interfaz.
    pub fn execute_add_mod(
        &mut self,
        event_tx: &tokio::sync::mpsc::Sender<crate::tui::event::AppEvent>,
    ) {
        let query = self.add_mod_input.value().trim().to_string();
        if query.is_empty() {
            self.add_mod_error = Some("Ingresa una URL o identificador válido".to_string());
            return;
        }

        let inst = match self.selected() {
            Some(i) => i.clone(),
            None => {
                self.add_mod_error = Some("No hay ningún servidor seleccionado".to_string());
                return;
            }
        };

        let subfolder = inst.addons_subfolder();
        let target_dir = self.paths.instance_dir(&inst.name).join(subfolder);

        self.add_mod_loading = true;
        self.add_mod_error = None;

        let tx = event_tx.clone();
        tokio::spawn(async move {
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

            let res = crate::loaders::serverpack::add_addon_from_url_or_slug(
                &query,
                &inst.mc_version,
                &inst.loader,
                &target_dir,
                &reporter,
            )
            .await;

            progress_task.abort();

            let _ = tx
                .send(crate::tui::event::AppEvent::AddModCompleted(
                    res.map_err(|e| e.to_string()),
                ))
                .await;
        });
    }

    /// Consulta asíncronamente las versiones de Minecraft soportadas por el modloader seleccionado.
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

    /// Inicia la resolución de metadatos remotos para una URL o slug de modpack.
    pub fn trigger_resolve_modpack(
        &mut self,
        event_tx: &tokio::sync::mpsc::Sender<crate::tui::event::AppEvent>,
    ) {
        let query = self.create_modpack_input.value().trim().to_string();
        if query.is_empty() || self.create_modpack_resolving {
            return;
        }

        if let Some(ref res) = self.resolved_modpack {
            if query == res.version_id
                || query.contains(&res.version_id)
                || query == res.suggested_instance_name
            {
                return;
            }
        }

        let is_sp = crate::loaders::serverpack::is_curseforge_or_zip(&query);
        self.create_modpack_resolving = true;
        self.create_modpack_status = Some(if is_sp {
            "Consultando CurseForge / Server Pack...".to_string()
        } else {
            "Consultando Modrinth...".to_string()
        });
        self.create_error = None;

        let tx = event_tx.clone();
        tokio::spawn(async move {
            let res = crate::modrinth::modpack::resolve_modpack(&query).await;
            let _ = tx
                .send(crate::tui::event::AppEvent::ModpackResolved(Box::new(
                    res.map_err(|e| e.to_string()),
                )))
                .await;
        });
    }

    /// Actualiza los campos del asistente de creación tras recibir metadatos de un modpack resuelto.
    pub fn handle_modpack_resolved(
        &mut self,
        res: Result<crate::modrinth::modpack::ResolvedModpack, String>,
    ) {
        self.create_modpack_resolving = false;
        match res {
            Ok(info) => {
                if self.create_name_input.value().trim().is_empty() {
                    self.create_name_input =
                        tui_input::Input::new(info.suggested_instance_name.clone());
                }
                let type_desc = match info.pack_type {
                    crate::modrinth::modpack::PackType::ServerPackZip => "Serverpack",
                    crate::modrinth::modpack::PackType::ModrinthMrpack => "Modpack",
                };
                self.create_modpack_status = Some(format!(
                    "✓ {} ({}: MC {}, {})",
                    info.project_title, type_desc, info.mc_version, info.loader
                ));
                self.set_status(&format!(
                    "{} detectado: {} (MC {}, {})",
                    type_desc, info.project_title, info.mc_version, info.loader
                ));
                self.resolved_modpack = Some(info);
            }
            Err(e) => {
                self.create_modpack_status = None;
                self.create_error = Some(format!("Error al resolver modpack: {}", e));
            }
        }
    }

    /// Ejecuta la creación, descarga e inicialización asíncrona de una nueva instancia de servidor.
    pub(crate) fn execute_create_instance(
        &mut self,
        event_tx: &tokio::sync::mpsc::Sender<crate::tui::event::AppEvent>,
    ) {
        let is_modpack = self.create_loader_idx == 3;
        let mut name = self.create_name_input.value().trim().to_string();

        if is_modpack && name.is_empty() {
            if let Some(ref res) = self.resolved_modpack {
                name = res.suggested_instance_name.clone();
                self.create_name_input = tui_input::Input::new(name.clone());
            }
        }

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

        let ram_val = self.create_ram_input.value().trim();
        let ram = if ram_val.is_empty() {
            "4G".to_string()
        } else {
            ram_val.to_string()
        };

        let port_val = self.create_port_input.value().trim();
        let port: u16 = if port_val.is_empty() {
            25565
        } else {
            match port_val.parse() {
                Ok(p) if p > 1024 => p,
                _ => {
                    self.create_error =
                        Some("Puerto TCP inválido (debe ser un número > 1024)".to_string());
                    return;
                }
            }
        };

        if is_modpack {
            let modpack_query = self.create_modpack_input.value().trim().to_string();
            if modpack_query.is_empty() && self.resolved_modpack.is_none() {
                self.create_error =
                    Some("Ingresa la URL de CurseForge, Modrinth o enlace .zip".to_string());
                return;
            }

            self.create_error = None;
            self.create_progress = Some((0.0, format!("Instalando modpack '{}'...", name)));

            let paths = self.paths.clone();
            let tx = event_tx.clone();
            let resolved = self.resolved_modpack.clone();

            tokio::spawn(async move {
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

                let res = crate::modrinth::modpack::install_modpack(
                    &name,
                    &modpack_query,
                    resolved,
                    &ram,
                    port,
                    &paths,
                    &reporter,
                )
                .await;

                progress_forwarder.abort();

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

    /// Elimina los archivos de la instancia, cerrando procesos remanentes y limpiando streams.
    pub fn execute_delete_instance(&mut self, name: &str) {
        let pid_file = self.paths.pid_file(name);
        if pid_file.exists() {
            if let Ok(content) = std::fs::read_to_string(&pid_file) {
                for line in content.lines() {
                    if let Ok(pid) = line.trim().parse::<u32>() {
                        if is_pid_alive(pid) {
                            #[cfg(unix)]
                            unsafe {
                                libc::kill(pid as libc::pid_t, libc::SIGTERM);
                            }
                        }
                    }
                }
            }
            let _ = std::fs::remove_file(&pid_file);
        }

        let sock = self.paths.socket_path(name);
        if sock.exists() {
            let _ = std::fs::remove_file(&sock);
        }

        let inst_dir = self.paths.instance_dir(name);
        if inst_dir.exists() {
            if let Err(e) = std::fs::remove_dir_all(&inst_dir) {
                self.set_status(&format!("Error al eliminar '{}': {}", name, e));
                return;
            }
        }

        if self.active_console_instance.as_deref() == Some(name) {
            self.abort_console_stream();
            self.log_buffer.clear();
            self.console_scroll = 0;
            self.active_console_instance = None;
        }

        if let Some(pos) = self.instances.iter().position(|i| i.name == name) {
            self.instances.remove(pos);
            if self.instances.is_empty() {
                self.selected_instance = 0;
                self.properties_list.clear();
            } else if self.selected_instance >= self.instances.len() {
                self.selected_instance = self.instances.len() - 1;
            }
        }

        self.load_properties_for_selected();
        if self.active_tab == AppTab::Console {
            self.start_console_stream();
        }

        self.set_status(&format!("✓ Servidor '{}' eliminado", name));
    }

}
