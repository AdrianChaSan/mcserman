pub mod action;
pub mod app;
pub mod event;
pub mod theme;
pub mod ui;
pub mod views;

use anyhow::{Context, Result};
use crossterm::cursor::Show;
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::prelude::CrosstermBackend;
use ratatui::Terminal;
use std::io::stdout;
use std::time::Duration;

use crate::core::paths::SystemPaths;
use crate::core::progress::ProgressUpdate;
use crate::tui::app::{App, InputMode};
use crate::tui::event::{spawn_event_loop, AppEvent, EventHandler};

/// Guardián de terminal que garantiza la restauración de raw_mode y alternate screen
/// incluso si una función retorna error o sale anticipadamente.
pub struct TerminalGuard {
    _private: (),
}

impl TerminalGuard {
    pub fn enter() -> Result<Self> {
        enable_raw_mode().context("Fallo al habilitar raw mode en la terminal")?;
        execute!(stdout(), EnterAlternateScreen).context("Fallo al entrar a alternate screen")?;
        Ok(Self { _private: () })
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = execute!(stdout(), LeaveAlternateScreen, Show);
        let _ = disable_raw_mode();
    }
}

/// Configura un panic hook que restaura la terminal antes de imprimir el panic backtrace.
fn setup_panic_hook() {
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = execute!(stdout(), LeaveAlternateScreen, Show);
        let _ = disable_raw_mode();
        original_hook(panic_info);
    }));
}

/// Punto de entrada principal para el servicio interactivo TUI.
pub async fn run(paths: &SystemPaths) -> Result<()> {
    setup_panic_hook();

    let _guard = TerminalGuard::enter()?;
    let backend = CrosstermBackend::new(stdout());
    let mut terminal =
        Terminal::new(backend).context("Fallo al inicializar terminal de Ratatui")?;

    let mut app = App::new(paths.clone());
    if let Ok(size) = terminal.size() {
        app.terminal_size = (size.width, size.height);
    }
    app.refresh_online_status().await;

    let mut log_rx = app.log_rx.take().expect("log_rx must be initialized");

    let mut event_handler = EventHandler::new(256);
    let event_tx = event_handler.sender();
    spawn_event_loop(event_tx.clone(), Duration::from_millis(100));

    // Iniciar streaming de consola si la pestaña inicial lo requiere
    if app.active_tab == app::AppTab::Console {
        app.start_console_stream();
    }

    let mut tick_counter: u32 = 0;

    // Render inicial
    terminal.draw(|f| ui::draw(f, &app))?;

    while !app.should_quit {
        tokio::select! {
            maybe_event = event_handler.next() => {
                let Some(event) = maybe_event else { break; };
                match event {
                    AppEvent::Key(key) => {
                        if let Some(action) = app.handle_key(key) {
                            app.update(action, &event_tx);
                        }
                    }
                    AppEvent::Resize(w, h) => {
                        app.update(action::Action::Resize(w, h), &event_tx);
                    }
                    AppEvent::Tick => {
                        app.check_status_expiration();
                        tick_counter = tick_counter.wrapping_add(1);
                        // Actualizar estado de las instancias cada 5 ticks (500 ms)
                        if tick_counter.is_multiple_of(5) {
                            app.refresh_online_status().await;
                        }
                    }
                    AppEvent::LogLineReceived { instance, line } => {
                        app.handle_log_line(&instance, line);
                    }
                    AppEvent::Progress(progress) => match progress {
                        ProgressUpdate::Info(msg) => {
                            app.set_status(&msg);
                        }
                        ProgressUpdate::Progress {
                            current_bytes,
                            total_bytes,
                        } => {
                            if total_bytes > 0 {
                                let pct = (current_bytes as f64 / total_bytes as f64) * 100.0;
                                if app.show_create_modal {
                                    app.create_progress = Some((
                                        pct,
                                        format!(
                                            "{:.1}% ({} de {} bytes)",
                                            pct, current_bytes, total_bytes
                                        ),
                                    ));
                                } else if app.active_tab == app::AppTab::Modrinth {
                                    app.modrinth_progress = Some((
                                        pct,
                                        format!(
                                            "{:.1}% ({} de {} bytes)",
                                            pct, current_bytes, total_bytes
                                        ),
                                    ));
                                }
                            }
                        }
                        ProgressUpdate::Started { title, .. } => {
                            if app.show_create_modal {
                                app.create_progress = Some((0.0, title));
                            } else if app.active_tab == app::AppTab::Modrinth {
                                app.modrinth_progress = Some((0.0, title));
                            }
                        }
                        ProgressUpdate::Finished { message } => {
                            app.set_status(&format!("✓ {}", message));
                        }
                        ProgressUpdate::Error(err) => {
                            app.set_status(&format!("✗ {}", err));
                        }
                    },
                    AppEvent::InstanceStatusChanged { name, state } => {
                        if let Some(inst) = app.instances.iter_mut().find(|i| i.name == name) {
                            inst.state = state;
                        }
                    }
                    AppEvent::AddModCompleted(res) => {
                        app.add_mod_loading = false;
                        app.modrinth_progress = None;
                        match res {
                            Ok(filename) => {
                                app.show_add_mod_modal = false;
                                app.input_mode = InputMode::Normal;
                                app.load_installed_mods();
                                app.set_status(&format!(
                                    "✓ '{}' instalado correctamente",
                                    filename
                                ));
                            }
                            Err(err) => {
                                app.add_mod_error = Some(err.clone());
                                app.set_status(&format!("✗ Error al instalar extensión: {}", err));
                            }
                        }
                    }
                    AppEvent::VersionsLoaded(vers) => {
                        let default_idx = vers
                            .iter()
                            .position(|v| {
                                v.starts_with("1.")
                                    && v[2..].chars().all(|c| c.is_ascii_digit() || c == '.')
                            })
                            .unwrap_or(0);
                        app.create_versions = vers;
                        app.create_loading_versions = false;
                        app.create_version_idx = default_idx;
                    }
                    AppEvent::VersionsLoadError(err) => {
                        app.create_error = Some(format!("Error cargando versiones: {}", err));
                        app.create_loading_versions = false;
                    }
                    AppEvent::ModpackResolved(res) => {
                        app.handle_modpack_resolved(*res);
                    }
                    AppEvent::InstanceCreated(res) => {
                        app.create_progress = None;
                        match res {
                            Ok(name) => {
                                app.show_create_modal = false;
                                app.input_mode = InputMode::Normal;
                                app.reload_instances();
                                app.set_status(&format!(
                                    "✓ ¡Instancia '{}' creada exitosamente!",
                                    name
                                ));
                            }
                            Err(err) => {
                                app.create_error = Some(err);
                            }
                        }
                    }
                    AppEvent::StatusMessage(msg) => {
                        app.set_status(&msg);
                    }
                }
            }
            maybe_log = log_rx.recv() => {
                if let Some((instance, line)) = maybe_log {
                    app.handle_log_line(&instance, line);
                }
            }
        }

        // Drenado por lotes no bloqueante: volcar hasta 500 líneas pendientes de log_rx
        let mut drained = 0;
        while drained < 500 {
            match log_rx.try_recv() {
                Ok((instance, line)) => {
                    app.handle_log_line(&instance, line);
                    drained += 1;
                }
                Err(_) => break,
            }
        }

        // Redibujar un único frame tras procesar eventos y el lote de logs
        terminal.draw(|f| ui::draw(f, &app))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::backend::TestBackend;

    #[test]
    fn test_tui_render_dashboard_does_not_panic() {
        let paths = SystemPaths::resolve();
        let app = App::new(paths);

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        let res = terminal.draw(|f| ui::draw(f, &app));
        assert!(
            res.is_ok(),
            "El renderizado base de la TUI no debe provocar panics"
        );

        let buffer = terminal.backend().buffer();
        let content: String = buffer.content().iter().map(|cell| cell.symbol()).collect();

        assert!(
            content.contains("mcserman"),
            "El render debe contener el título de mcserman"
        );
    }

    #[test]
    fn test_tui_small_screen_warning() {
        let paths = SystemPaths::resolve();
        let app = App::new(paths);

        // Terminal menor al tamaño mínimo permitido
        let backend = TestBackend::new(40, 10);
        let mut terminal = Terminal::new(backend).unwrap();

        let res = terminal.draw(|f| ui::draw(f, &app));
        assert!(res.is_ok());

        let buffer = terminal.backend().buffer();
        let content: String = buffer.content().iter().map(|cell| cell.symbol()).collect();

        assert!(
            content.contains("demasiado peque"),
            "Debe mostrar aviso de tamaño mínimo"
        );
    }

    #[test]
    fn test_input_mode_insert_blocks_quit_and_global_shortcuts() {
        let paths = SystemPaths::resolve();
        let mut app = App::new(paths);

        // En modo Normal, 'q' debe emitir Action::Quit
        let key_q = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        assert_eq!(app.handle_key(key_q), Some(action::Action::Quit));

        // Entrar en modo Insert (ej. en la consola)
        app.input_mode = InputMode::Insert;
        app.active_tab = app::AppTab::Console;

        // En modo Insert, 'q' NO debe emitir Action::Quit, sino escribir 'q' en el input
        let action = app.handle_key(key_q);
        assert_eq!(action, None, "Modo Insert debe bloquear Action::Quit");
        assert_eq!(
            app.console_input.value(),
            "q",
            "La tecla 'q' debe haberse insertado en el buffer de texto"
        );

        // Espacio NO debe emitir ToggleServer
        let key_space = KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE);
        assert_eq!(app.handle_key(key_space), None);
        assert_eq!(app.console_input.value(), "q ");

        // Esc debe volver a modo Normal
        let key_esc = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        assert_eq!(app.handle_key(key_esc), None);
        assert_eq!(
            app.input_mode,
            InputMode::Normal,
            "Esc debe retornar a InputMode::Normal"
        );
    }

    #[test]
    fn test_console_ring_buffer_bounded_capacity() {
        let paths = SystemPaths::resolve();
        let mut app = App::new(paths);

        // Insertar más líneas que la capacidad máxima del ring buffer
        for i in 0..2000 {
            app.push_log_line(format!("[INFO] Registro de prueba número {}", i));
        }

        assert_eq!(
            app.log_buffer.len(),
            app::MAX_LOG_LINES,
            "El ring buffer de logs debe acotarse estrictamente a MAX_LOG_LINES ({})",
            app::MAX_LOG_LINES
        );

        // La línea más antigua debe haber sido descartada
        let first = app.log_buffer.front().unwrap();
        assert!(
            first.contains("1000"),
            "Las líneas más antiguas deben haberse descartado primero"
        );
    }

    #[test]
    fn test_console_sanitization_strips_ansi_and_tabs() {
        let raw = "\x1B[32m[INFO]\x1B[0m Servidor arrancado\tOK\r\n";
        let clean = app::sanitize_log_line(raw);
        assert!(!clean.contains('\x1B'), "No debe contener escapes ANSI");
        assert!(!clean.contains('\t'), "Los tabs deben haberse expandido");
        assert!(
            !clean.contains('\r'),
            "Los retornos de carro deben haberse eliminado"
        );
        assert!(clean.contains("[INFO] Servidor arrancado    OK\n"));
    }

    #[test]
    fn test_modal_and_tabs_render_without_panic() {
        let paths = SystemPaths::resolve();
        let mut app = App::new(paths);

        let backend = TestBackend::new(90, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        // 1. Probar renderizado de pestaña Consola
        app.active_tab = app::AppTab::Console;
        app.push_log_line("[INFO] Iniciando Minecraft".to_string());
        app.push_log_line("[WARN] Lag detectado en chunk".to_string());
        app.push_log_line("[ERROR] Crash simulado".to_string());
        assert!(terminal.draw(|f| ui::draw(f, &app)).is_ok());

        // 2. Probar renderizado de pestaña Modrinth
        app.active_tab = app::AppTab::Modrinth;
        assert!(terminal.draw(|f| ui::draw(f, &app)).is_ok());

        // 3. Probar renderizado de pestaña Propiedades
        app.active_tab = app::AppTab::Settings;
        assert!(terminal.draw(|f| ui::draw(f, &app)).is_ok());

        // 4. Probar renderizado de Modal de Creación
        app.show_create_modal = true;
        assert!(terminal.draw(|f| ui::draw(f, &app)).is_ok());

        let buffer = terminal.backend().buffer();
        let content: String = buffer.content().iter().map(|cell| cell.symbol()).collect();
        assert!(
            content.contains("Crear Nueva Instancia"),
            "El modal debe ser visible en el buffer"
        );
    }

    #[test]
    fn test_parse_minecraft_formatting() {
        use ratatui::style::Color;

        let raw = "§cRojo §aVerde §lNegrita";
        let spans = views::properties::parse_minecraft_formatting(raw);

        assert_eq!(spans.len(), 3);
        assert_eq!(spans[0].content, "Rojo ");
        assert_eq!(spans[0].style.fg, Some(Color::LightRed));

        assert_eq!(spans[1].content, "Verde ");
        assert_eq!(spans[1].style.fg, Some(Color::LightGreen));

        assert_eq!(spans[2].content, "Negrita");
        assert!(spans[2]
            .style
            .add_modifier
            .contains(ratatui::style::Modifier::BOLD));
    }

    #[test]
    fn test_modal_esc_disambiguation() {
        let paths = SystemPaths::resolve();
        let mut app = App::new(paths);

        // Abrir modal de creación (inicia en Insert)
        app.show_create_modal = true;
        app.input_mode = InputMode::Insert;

        let key_esc = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);

        // Primer Esc: debe salir de Insert a Normal SIN cerrar el modal
        let act1 = app.handle_key(key_esc);
        assert_eq!(act1, None);
        assert_eq!(app.input_mode, InputMode::Normal);
        assert!(app.show_create_modal);

        // Segundo Esc: ahora en Normal, debe cerrar el modal
        let act2 = app.handle_key(key_esc);
        assert_eq!(act2, Some(action::Action::CloseCreateModal));
    }

    #[test]
    fn test_focus_switching() {
        let paths = SystemPaths::resolve();
        let mut app = App::new(paths);

        // Dashboard inicia con foco en Sidebar
        app.active_tab = app::AppTab::Dashboard;
        app.focus = app::Focus::Sidebar;

        // 'l' / Right pasa foco al panel principal
        let key_right = KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE);
        let act = app.handle_key(key_right);
        assert_eq!(act, Some(action::Action::FocusMainPanel));
        app.update(act.unwrap(), &tokio::sync::mpsc::channel(1).0);
        assert_eq!(app.focus, app::Focus::MainPanel);

        // 'h' / Left regresa foco a la sidebar
        let key_left = KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE);
        let act = app.handle_key(key_left);
        assert_eq!(act, Some(action::Action::FocusSidebar));
        app.update(act.unwrap(), &tokio::sync::mpsc::channel(1).0);
        assert_eq!(app.focus, app::Focus::Sidebar);
    }

    #[test]
    fn test_console_visual_lines_and_clamping() {
        let paths = SystemPaths::resolve();
        let mut app = App::new(paths);
        app.terminal_size = (80, 24);
        let (tx, _) = tokio::sync::mpsc::channel(16);

        // Sin logs: total visual lines es 0, max_scroll es 0
        assert_eq!(app.console_total_visual_lines(), 0);
        assert_eq!(app.console_max_scroll(), 0);

        // Intentar scrollear arriba sin logs no debe incrementar console_scroll
        app.update(action::Action::ScrollLogUp(15), &tx);
        assert_eq!(app.console_scroll, 0);

        // inner_width = 80 - 26 (sidebar) - 2 (border) = 52
        // Agregar líneas cortas
        for i in 0..10 {
            app.push_log_line(format!("Línea corta {}", i));
        }
        // 10 líneas cortas <= viewport_height (12), max_scroll debe ser 0
        assert_eq!(app.console_total_visual_lines(), 10);
        assert_eq!(app.console_max_scroll(), 0);
        app.update(action::Action::ScrollLogUp(5), &tx);
        assert_eq!(app.console_scroll, 0);

        // Agregar 30 líneas más para superar viewport_height
        for i in 10..40 {
            app.push_log_line(format!("Línea de log estándar número {}", i));
        }
        let total = app.console_total_visual_lines();
        assert_eq!(total, 40);
        let max_scroll = app.console_max_scroll();
        assert!(max_scroll > 0);

        // Scrollear hacia arriba: debe saturar en max_scroll, nunca superarlo
        app.update(action::Action::ScrollLogUp(9999), &tx);
        assert_eq!(app.console_scroll, max_scroll);
        assert!(!app.auto_scroll);

        // Scrollear hacia abajo parcialmente
        app.update(action::Action::ScrollLogDown(5), &tx);
        assert_eq!(app.console_scroll, max_scroll - 5);
        assert!(!app.auto_scroll);

        // Scrollear al final (ScrollLogBottom)
        app.update(action::Action::ScrollLogBottom, &tx);
        assert_eq!(app.console_scroll, 0);
        assert!(app.auto_scroll);
    }

    #[test]
    fn test_console_render_scroll_and_scrollbar() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let paths = SystemPaths::resolve();
        let mut app = App::new(paths);
        app.active_tab = app::AppTab::Console;
        app.focus = app::Focus::MainPanel;

        for i in 0..50 {
            app.push_log_line(format!("Log message {:03} with some extra padding text to test wrapping behavior across multiple columns", i));
        }

        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        // Render al final (auto_scroll = true)
        terminal.draw(|f| ui::draw(f, &app)).unwrap();

        // Scrollear hacia arriba y renderizar de nuevo con scrollbar
        app.console_scroll = 20;
        app.auto_scroll = false;
        terminal.draw(|f| ui::draw(f, &app)).unwrap();
    }

    #[tokio::test]
    async fn test_sidebar_server_state_and_log_trigger() {
        let paths = SystemPaths::resolve();
        let mut app = App::new(paths);
        app.instances.clear();
        app.instances.push(app::InstanceSummary {
            name: "test_srv".to_string(),
            loader: "paper".to_string(),
            mc_version: "1.21".to_string(),
            ram: "4G".to_string(),
            port: 25565,
            state: app::ServerState::Offline,
            source_pack: None,
        });
        app.selected_instance = 0;

        // Si la instancia recibe logs en vivo, no puede estar Offline -> debe pasar a Starting
        if let Some(inst) = app.instances.iter_mut().find(|i| i.name == "test_srv") {
            if inst.state == app::ServerState::Offline {
                inst.state = app::ServerState::Starting;
            }
        }
        assert_eq!(app.instances[0].state, app::ServerState::Starting);

        // Toggle server reactivo inmediato en memoria
        let (tx, _) = tokio::sync::mpsc::channel(16);
        app.instances[0].state = app::ServerState::Offline;
        app.execute_toggle_server(&tx);
        assert_eq!(app.instances[0].state, app::ServerState::Starting);
    }

    #[tokio::test]
    async fn test_status_message_ttl_and_navigation_clearing() {
        let paths = SystemPaths::resolve();
        let mut app = App::new(paths);
        let (tx, _) = tokio::sync::mpsc::channel(16);

        // 1. Asignar mensaje de estado
        app.set_status("Servidor 'test' ONLINE.");
        assert!(app.status_message.is_some());

        // Comprobación inmediata no debe expirar
        app.check_status_expiration();
        assert!(app.status_message.is_some());

        // 2. Simular expiración por tiempo (> 3s)
        app.status_message = Some((
            "Servidor 'test' ONLINE.".to_string(),
            std::time::Instant::now() - std::time::Duration::from_secs(4),
        ));
        app.check_status_expiration();
        assert!(app.status_message.is_none());

        // 3. Limpieza inmediata al navegar entre pestañas
        app.set_status("Mensaje temporal");
        assert!(app.status_message.is_some());
        app.update(action::Action::NextTab, &tx);
        assert!(app.status_message.is_none());

        app.set_status("Mensaje temporal 2");
        app.update(action::Action::GoToTab(0), &tx);
        assert!(app.status_message.is_none());

        // 4. Limpieza al cambiar foco
        app.set_status("Mensaje temporal 3");
        app.update(action::Action::FocusMainPanel, &tx);
        assert!(app.status_message.is_none());
    }

    #[test]
    fn test_sanitize_log_line_carriage_return_and_jline_erase() {
        // Test de reescritura por retorno de carro \r (progreso)
        let progress = "Downloading Paper 10%\rDownloading Paper 100%\r\n";
        let clean_prog = app::sanitize_log_line(progress);
        assert_eq!(clean_prog, "Downloading Paper 100%\n");

        // Test de secuencias de borrado de línea JLine/Paper (\r\x1b[2K)
        let jline = "\r\x1b[2K[12:00:00 INFO]: Servidor ONLINE\r\n";
        let clean_jline = app::sanitize_log_line(jline);
        assert_eq!(clean_jline, "[12:00:00 INFO]: Servidor ONLINE\n");

        // Test de \x1b[K (borrado hacia adelante)
        let forward_erase = "Long line from before that was longer\r\x1b[KShort line";
        let clean_erase = app::sanitize_log_line(forward_erase);
        assert_eq!(clean_erase, "Short line");
    }

    #[test]
    fn test_simplified_create_modal_labels() {
        let paths = SystemPaths::resolve();
        let mut app = App::new(paths);
        app.show_create_modal = true;

        let backend = ratatui::backend::TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        assert!(terminal.draw(|f| ui::draw(f, &app)).is_ok());

        let buffer = terminal.backend().buffer();
        let content: String = buffer.content().iter().map(|cell| cell.symbol()).collect();

        // 1. Verificar títulos y etiquetas simplificadas
        assert!(
            content.contains("Tipo de Servidor"),
            "Debe contener 'Tipo de Servidor'"
        );
        assert!(
            content.contains("Versión de Minecraft"),
            "Debe contener 'Versión de Minecraft'"
        );
        assert!(
            content.contains("Memoria RAM"),
            "Debe contener 'Memoria RAM'"
        );
        assert!(content.contains("Puerto"), "Debe contener 'Puerto'");
        assert!(content.contains("[● Paper]"), "Debe contener '[● Paper]'");
        assert!(content.contains("○ Fabric"), "Debe contener '○ Fabric'");
        assert!(content.contains("○ Vanilla"), "Debe contener '○ Vanilla'");

        // 2. Verificar que se eliminó el ruido visual previo
        assert!(
            !content.contains("(Modloader)"),
            "No debe contener '(Modloader)'"
        );
        assert!(
            !content.contains("Flechas ◄ ►"),
            "No debe contener 'Flechas ◄ ►' en títulos"
        );
        assert!(
            !content.contains("(Recomendado)"),
            "No debe contener '(Recomendado)'"
        );
        assert!(!content.contains("(Mods)"), "No debe contener '(Mods)'");
        assert!(
            !content.contains("Memoria RAM máxima"),
            "No debe contener 'Memoria RAM máxima'"
        );
        assert!(
            !content.contains("Puerto TCP del servidor"),
            "No debe contener 'Puerto TCP del servidor'"
        );
    }

    #[test]
    fn test_delete_server_flow_and_index_adjustment() {
        let unique_id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let temp_dir = std::env::temp_dir().join(format!("mcserman_test_{}", unique_id));
        let paths = SystemPaths {
            instances_dir: temp_dir.join("instances"),
            runtimes_dir: temp_dir.join("runtimes"),
            socket_dir: temp_dir.join("run"),
            base_dir: temp_dir.clone(),
        };
        paths.ensure_dirs().unwrap();

        let mut app = App::new(paths.clone());
        let (tx, _rx) = tokio::sync::mpsc::channel(32);

        // Crear 3 directorios simulados de instancias
        for name in &["alpha", "beta", "gamma"] {
            let inst_dir = paths.instance_dir(name);
            std::fs::create_dir_all(&inst_dir).unwrap();
            let cfg = crate::core::config::InstanceConfig::new(
                name.to_string(),
                crate::core::config::LoaderType::Paper,
                "1.21.4".to_string(),
            );
            cfg.save(&inst_dir.join("instance.toml")).unwrap();
        }

        app.reload_instances();
        assert_eq!(app.instances.len(), 3);
        assert_eq!(app.instances[0].name, "alpha");
        assert_eq!(app.instances[1].name, "beta");
        assert_eq!(app.instances[2].name, "gamma");

        // 1. Si el servidor está ONLINE o STARTING, impedir borrado
        app.selected_instance = 1; // "beta"
        app.instances[1].state = app::ServerState::Online;

        let key_d = KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE);
        let action = app.handle_key(key_d);
        assert_eq!(action, Some(action::Action::PromptDeleteInstance));

        app.update(action.unwrap(), &tx);
        assert!(
            !app.show_delete_modal,
            "No debe abrir el modal de borrado si está ONLINE"
        );
        assert!(
            app.status_message
                .as_ref()
                .map(|(m, _)| m.contains("Detén el servidor antes de eliminarlo"))
                .unwrap_or(false),
            "Debe advertir que se detenga el servidor"
        );

        // 2. Si el servidor está OFFLINE, permitir borrado
        app.instances[1].state = app::ServerState::Offline;
        let action_offline = app.handle_key(key_d);
        assert_eq!(action_offline, Some(action::Action::PromptDeleteInstance));
        app.update(action_offline.unwrap(), &tx);

        assert!(app.show_delete_modal, "Debe abrir modal de confirmación");
        assert_eq!(app.delete_server_name.as_deref(), Some("beta"));

        // 3. Probar cancelar modal con 'n' y 'Esc'
        let key_n = KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE);
        assert_eq!(
            app.handle_key(key_n),
            Some(action::Action::CancelDeleteInstance)
        );
        app.update(action::Action::CancelDeleteInstance, &tx);
        assert!(!app.show_delete_modal);
        assert!(app.delete_server_name.is_none());
        assert_eq!(app.instances.len(), 3);

        // 4. Probar atajo alternativo 'x' y Delete
        let key_x = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE);
        assert_eq!(
            app.handle_key(key_x),
            Some(action::Action::PromptDeleteInstance)
        );
        let key_del = KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE);
        assert_eq!(
            app.handle_key(key_del),
            Some(action::Action::PromptDeleteInstance)
        );

        // 5. Confirmar borrado de "beta" (índice 1 de [alpha, beta, gamma])
        app.update(action::Action::PromptDeleteInstance, &tx);
        assert!(app.show_delete_modal);

        let key_enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(
            app.handle_key(key_enter),
            Some(action::Action::ConfirmDeleteInstance)
        );
        app.update(action::Action::ConfirmDeleteInstance, &tx);

        assert!(!app.show_delete_modal);
        assert_eq!(app.instances.len(), 2);
        assert_eq!(app.instances[0].name, "alpha");
        assert_eq!(app.instances[1].name, "gamma");
        // El índice 1 ahora apunta a "gamma" (el siguiente servidor)
        assert_eq!(app.selected_instance, 1);
        assert!(
            !paths.instance_dir("beta").exists(),
            "El directorio de 'beta' debe haber sido eliminado"
        );

        // 6. Borrar "gamma" (último elemento, índice 1 de 2)
        app.update(action::Action::PromptDeleteInstance, &tx);
        app.update(action::Action::ConfirmDeleteInstance, &tx);

        assert_eq!(app.instances.len(), 1);
        assert_eq!(app.instances[0].name, "alpha");
        // El índice se ajusta al elemento previo (0)
        assert_eq!(app.selected_instance, 0);

        // 7. Borrar la última instancia ("alpha")
        app.update(action::Action::PromptDeleteInstance, &tx);
        app.update(action::Action::ConfirmDeleteInstance, &tx);

        assert_eq!(app.instances.len(), 0);
        assert_eq!(app.selected_instance, 0);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_delete_modal_rendering() {
        let paths = SystemPaths::resolve();
        let mut app = App::new(paths);
        app.show_delete_modal = true;
        app.delete_server_name = Some("survival-hardcore".to_string());

        let backend = ratatui::backend::TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        assert!(terminal.draw(|f| ui::draw(f, &app)).is_ok());

        let buffer = terminal.backend().buffer();
        let content: String = buffer.content().iter().map(|cell| cell.symbol()).collect();

        assert!(
            content.contains("¿Eliminar servidor 'survival-hardcore'?"),
            "Debe incluir la pregunta con el nombre del servidor"
        );
        assert!(
            content.contains("Esta acción borrará sus archivos."),
            "Debe advertir sobre el borrado de archivos"
        );
        assert!(
            content.contains("[ y / Enter ] Confirmar"),
            "Debe mostrar atajo de confirmación"
        );
        assert!(
            content.contains("[ n / Esc ] Cancelar"),
            "Debe mostrar atajo de cancelación"
        );
    }

    #[test]
    fn test_footer_contains_delete_hint() {
        let paths = SystemPaths::resolve();
        let mut app = App::new(paths);

        let backend = ratatui::backend::TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        // 1. En Dashboard con foco en Sidebar
        app.active_tab = app::AppTab::Dashboard;
        app.focus = app::Focus::Sidebar;
        assert!(terminal.draw(|f| ui::draw(f, &app)).is_ok());
        let buf1: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(
            buf1.contains("d: Eliminar"),
            "Dashboard Sidebar debe incluir 'd: Eliminar'"
        );

        // 2. En Dashboard con foco en MainPanel
        app.focus = app::Focus::MainPanel;
        assert!(terminal.draw(|f| ui::draw(f, &app)).is_ok());
        let buf2: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(
            buf2.contains("d: Eliminar"),
            "Dashboard MainPanel debe incluir 'd: Eliminar'"
        );

        // 3. En Consola con foco en Sidebar
        app.active_tab = app::AppTab::Console;
        app.focus = app::Focus::Sidebar;
        assert!(terminal.draw(|f| ui::draw(f, &app)).is_ok());
        let buf3: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(
            buf3.contains("d: Eliminar"),
            "Console Sidebar debe incluir 'd: Eliminar'"
        );

        // 4. Modal de eliminación activo
        app.show_delete_modal = true;
        assert!(terminal.draw(|f| ui::draw(f, &app)).is_ok());
        let buf4: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(
            buf4.contains("y/Enter: Confirmar eliminación"),
            "Footer debe reflejar modo de borrado"
        );
    }

    #[test]
    fn test_properties_common_vs_all_mode() {
        let paths = SystemPaths::resolve();
        let mut app = App::new(paths);
        let (tx, _rx) = tokio::sync::mpsc::channel(10);

        // Preparamos propiedades comunes y propiedades raras/avanzadas
        app.properties_list = vec![
            ("motd".to_string(), "Minecraft Server".to_string()),
            ("server-port".to_string(), "25565".to_string()),
            ("difficulty".to_string(), "easy".to_string()),
            ("rcon.port".to_string(), "25575".to_string()), // No común
            ("broadcast-rcon-to-ops".to_string(), "true".to_string()), // No común
        ];

        // 1. Por defecto (modo común): solo debe incluir motd, server-port, difficulty (3)
        assert!(!app.properties_show_all);
        let filtered_common = app.filtered_properties();
        assert_eq!(filtered_common.len(), 3);
        assert_eq!(filtered_common[0].1 .0, "motd");
        assert_eq!(filtered_common[1].1 .0, "server-port");
        assert_eq!(filtered_common[2].1 .0, "difficulty");

        // 2. Presionar 'a': toggle a modo Todas
        app.active_tab = app::AppTab::Settings;
        app.focus = app::Focus::MainPanel;
        let key_a = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
        let action = app.handle_key(key_a);
        assert_eq!(action, Some(action::Action::TogglePropertiesShowAll));
        app.update(action.unwrap(), &tx);

        assert!(app.properties_show_all);
        assert_eq!(app.filtered_properties().len(), 5);

        // 3. Regresar a modo Comunes con 'a'
        let action2 = app.handle_key(key_a);
        assert_eq!(action2, Some(action::Action::TogglePropertiesShowAll));
        app.update(action2.unwrap(), &tx);

        assert!(!app.properties_show_all);
        assert_eq!(app.filtered_properties().len(), 3);

        // 4. Búsqueda con / busca sobre TODAS independientemente del modo
        app.properties_search_input = tui_input::Input::new("rcon".to_string());
        let search_results = app.filtered_properties();
        assert_eq!(search_results.len(), 2);
        assert!(search_results.iter().any(|(_, (k, _))| k == "rcon.port"));
        assert!(search_results
            .iter()
            .any(|(_, (k, _))| k == "broadcast-rcon-to-ops"));
    }

    #[test]
    fn test_properties_search_filter_and_navigation() {
        let paths = SystemPaths::resolve();
        let mut app = App::new(paths);
        let (tx, _rx) = tokio::sync::mpsc::channel(10);

        app.properties_list = vec![
            ("server-port".to_string(), "25565".to_string()),
            ("online-mode".to_string(), "true".to_string()),
            ("motd".to_string(), "A Minecraft Server".to_string()),
            ("difficulty".to_string(), "easy".to_string()),
            ("gamemode".to_string(), "survival".to_string()),
        ];

        // 1. Sin filtro devuelve todos
        assert_eq!(app.filtered_properties().len(), 5);

        // 2. Filtro por clave insensible a mayúsculas
        app.properties_search_input = tui_input::Input::new("PORT".to_string());
        let filtered = app.filtered_properties();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].1 .0, "server-port");

        // 3. Filtro por valor
        app.properties_search_input = tui_input::Input::new("SURVIVAL".to_string());
        let filtered_val = app.filtered_properties();
        assert_eq!(filtered_val.len(), 1);
        assert_eq!(filtered_val[0].1 .0, "gamemode");

        // 4. Flujo interactivo: Activar búsqueda
        app.active_tab = app::AppTab::Settings;
        app.focus = app::Focus::MainPanel;
        app.properties_search_input.reset();

        let slash_key = KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE);
        let action = app.handle_key(slash_key);
        assert_eq!(action, Some(action::Action::StartPropertiesSearch));
        app.update(action.unwrap(), &tx);
        assert!(app.properties_search_active);

        // 5. Cancelar con Esc
        let esc_key = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        let action_esc = app.handle_key(esc_key);
        assert_eq!(action_esc, Some(action::Action::CancelPropertiesSearch));
        app.update(action_esc.unwrap(), &tx);
        assert!(!app.properties_search_active);
        assert!(app.properties_search_input.value().is_empty());
    }

    #[test]
    fn test_properties_tab_full_width_rendering() {
        let paths = SystemPaths::resolve();
        let mut app = App::new(paths);
        let (tx, _rx) = tokio::sync::mpsc::channel(10);

        app.active_tab = app::AppTab::Settings;
        app.focus = app::Focus::MainPanel;
        app.properties_list = vec![
            ("motd".to_string(), "Minecraft Server".to_string()),
            ("server-port".to_string(), "25565".to_string()),
            ("view-distance".to_string(), "10".to_string()),
        ];
        app.selected_property = 0;

        let backend = ratatui::backend::TestBackend::new(140, 35);
        let mut terminal = Terminal::new(backend).unwrap();

        // 1. Render en modo [Comunes] (por defecto)
        assert!(terminal.draw(|f| ui::draw(f, &app)).is_ok());
        let content: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();

        // Verificar que NO hay panel derecho "Detalles de la Propiedad"
        assert!(
            !content.contains("Detalles de la Propiedad"),
            "El panel derecho debe haber sido removido"
        );
        // Verificar título en modo común
        assert!(
            content.contains("[Comunes] (Presiona 'a' para ver todas)"),
            "Debe indicar modo Comunes en el título"
        );

        // 2. Cambiar a modo [Todas] y verificar título
        app.update(action::Action::TogglePropertiesShowAll, &tx);
        assert!(terminal.draw(|f| ui::draw(f, &app)).is_ok());
        let content_all: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(
            content_all.contains("[Todas] (Presiona 'a' para filtrar comunes)"),
            "Debe indicar modo Todas en el título"
        );

        // 3. Verificar Footer con el nuevo atajo 'a: Todas/Comunes' y los demás requeridos
        assert!(
            content.contains("q: Salir"),
            "Footer debe contener 'q: Salir'"
        );
        assert!(
            content.contains("a: Todas/Comunes"),
            "Footer debe contener 'a: Todas/Comunes'"
        );
        assert!(
            content.contains("/: Buscar"),
            "Footer debe contener '/: Buscar'"
        );
        assert!(
            content.contains("Space: Alternar"),
            "Footer debe contener 'Space: Alternar'"
        );
        assert!(
            content.contains("Enter: Editar"),
            "Footer debe contener 'Enter: Editar'"
        );
        assert!(
            content.contains("s: Guardar"),
            "Footer debe contener 's: Guardar'"
        );
    }

    #[tokio::test]
    async fn test_create_modal_modpack_ui_and_resolution() {
        let paths = SystemPaths::resolve();
        let mut app = App::new(paths);
        let (tx, _rx) = tokio::sync::mpsc::channel(10);

        // Abrir modal de creación
        app.update(action::Action::OpenCreateModal, &tx);
        assert!(app.show_create_modal);
        assert_eq!(app.create_loader_idx, 0);

        // Seleccionar loader Modpack (índice 3)
        app.update(action::Action::SelectWizardLoader(3), &tx);
        assert_eq!(app.create_loader_idx, 3);

        // Renderizar modal en TestBackend y verificar labels dinámicos
        let backend = ratatui::backend::TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        assert!(terminal.draw(|f| ui::draw(f, &app)).is_ok());

        let buffer_content: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();

        // Debe contener la opción Modpack en el selector
        assert!(
            buffer_content.contains("Modpack"),
            "El selector debe mostrar la opción Modpack"
        );
        // Debe mostrar el label dinámico de modpack en lugar de Versión de Minecraft
        assert!(
            buffer_content.contains("Modpack / Server Pack"),
            "Debe mostrar el prompt para CurseForge, Modrinth o .zip"
        );

        // Simular resolución exitosa de Modpack
        let mock_resolved = crate::modrinth::modpack::ResolvedModpack {
            pack_type: crate::modrinth::modpack::PackType::ModrinthMrpack,
            project_id: "better-mc".to_string(),
            project_title: "Better MC [FABRIC] BMC2".to_string(),
            version_id: "M5BnAIQy".to_string(),
            version_name: "v40".to_string(),
            suggested_instance_name: "better-mc-fabric-bmc2".to_string(),
            mc_version: "1.20.1".to_string(),
            loader: "fabric".to_string(),
            loader_version: Some("0.16.9".to_string()),
            mrpack_url: "https://example.com/test.mrpack".to_string(),
            mrpack_filename: "test.mrpack".to_string(),
            mrpack_sha512: None,
            mrpack_size: 1024,
        };

        app.handle_modpack_resolved(Ok(mock_resolved));

        // Verificar que autocompleta el nombre de la instancia si estaba vacío
        assert_eq!(app.create_name_input.value(), "better-mc-fabric-bmc2");
        // Verificar que genera el status de éxito
        assert!(app
            .create_modpack_status
            .as_ref()
            .unwrap()
            .contains("Better MC [FABRIC] BMC2"));
        assert!(app.resolved_modpack.is_some());

        // Renderizar nuevamente para verificar que el status se dibuja en pantalla
        assert!(terminal.draw(|f| ui::draw(f, &app)).is_ok());
        let updated_content: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();

        assert!(
            updated_content.contains("Better MC [FABRIC] BMC2"),
            "El estado resuelto debe aparecer visualmente en el modal"
        );
    }

    #[test]
    fn test_format_file_size() {
        use crate::tui::views::modrinth::format_file_size;
        assert_eq!(format_file_size(500), "500 B");
        assert_eq!(format_file_size(1024), "1.0 KB");
        assert_eq!(format_file_size(1536), "1.5 KB");
        assert_eq!(format_file_size(1024 * 1024), "1.0 MB");
        assert_eq!(format_file_size(5 * 1024 * 1024), "5.0 MB");
        assert_eq!(format_file_size(2 * 1024 * 1024 * 1024), "2.00 GB");
    }

    #[tokio::test]
    async fn test_mods_tab_serverpack_warning_banner() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let paths = SystemPaths::resolve();
        let mut app = App::new(paths);
        app.instances.clear();
        app.instances.push(app::InstanceSummary {
            name: "bmc2_srv".to_string(),
            loader: "fabric".to_string(),
            mc_version: "1.20.1".to_string(),
            ram: "6G".to_string(),
            port: 25565,
            state: app::ServerState::Offline,
            source_pack: Some("serverpack:Better MC [FABRIC] BMC2".to_string()),
        });
        app.selected_instance = 0;
        app.active_tab = app::AppTab::Modrinth;
        app.focus = app::Focus::MainPanel;

        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        assert!(terminal.draw(|f| ui::draw(f, &app)).is_ok());

        let buffer_content: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();

        assert!(
            buffer_content.contains("Advertencia"),
            "Debe mostrar el banner de advertencia si la instancia es de un Serverpack"
        );
        assert!(
            buffer_content.contains("Serverpack oficial"),
            "Debe explicar el riesgo de incompatibilidad/desincronización"
        );

        // Sin source_pack no debe mostrar la advertencia
        app.instances[0].source_pack = None;
        assert!(terminal.draw(|f| ui::draw(f, &app)).is_ok());
        let updated_content: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();

        assert!(
            !updated_content.contains("Serverpack oficial"),
            "No debe mostrar la advertencia si la instancia es estándar"
        );
    }

    #[tokio::test]
    async fn test_mods_tab_installed_and_navigation() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let paths = SystemPaths::resolve();
        let mut app = App::new(paths);
        let (tx, _rx) = tokio::sync::mpsc::channel(10);

        app.instances.clear();
        app.instances.push(app::InstanceSummary {
            name: "fabric_test".to_string(),
            loader: "fabric".to_string(),
            mc_version: "1.20.1".to_string(),
            ram: "4G".to_string(),
            port: 25565,
            state: app::ServerState::Offline,
            source_pack: None,
        });
        app.selected_instance = 0;
        app.active_tab = app::AppTab::Modrinth;
        app.focus = app::Focus::MainPanel;

        app.installed_mods = vec![
            app::InstalledMod {
                file_name: "fabric-api-0.92.jar".to_string(),
                size_bytes: 2 * 1024 * 1024,
            },
            app::InstalledMod {
                file_name: "sodium-fabric-0.5.8.jar".to_string(),
                size_bytes: 512 * 1024,
            },
        ];

        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        assert!(terminal.draw(|f| ui::draw(f, &app)).is_ok());

        let buffer_content: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();

        assert!(buffer_content.contains("fabric-api-0.92.jar"));
        assert!(buffer_content.contains("sodium-fabric-0.5.8.jar"));
        assert!(buffer_content.contains("2.0 MB"));
        assert!(buffer_content.contains("512.0 KB"));
        assert!(buffer_content.contains("Mods Instalados (2)"));

        // Navegación con Down y Up
        assert_eq!(app.selected_installed_mod, 0);
        app.update(action::Action::Down, &tx);
        assert_eq!(app.selected_installed_mod, 1);
        app.update(action::Action::Up, &tx);
        assert_eq!(app.selected_installed_mod, 0);

        // Probar scroll dinámico con 30 mods en terminal de altura reducida
        let backend_small = TestBackend::new(100, 20);
        let mut terminal_small = Terminal::new(backend_small).unwrap();

        app.installed_mods = (0..30)
            .map(|i| app::InstalledMod {
                file_name: format!("mod-{:02}.jar", i),
                size_bytes: 1024 * 1024,
            })
            .collect();
        app.selected_installed_mod = 0;

        // Al inicio debe renderizar mod-00.jar pero NO mod-25.jar
        terminal_small.draw(|f| ui::draw(f, &app)).unwrap();
        let buf0: String = terminal_small
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(buf0.contains("mod-00.jar"));
        assert!(!buf0.contains("mod-25.jar"));

        // Al navegar hacia abajo hasta mod-25
        for _ in 0..25 {
            app.update(action::Action::Down, &tx);
        }
        assert_eq!(app.selected_installed_mod, 25);

        // Al redibujar, la vista DEBE haberse desplazado (scrolling activo) y mostrar mod-25.jar
        terminal_small.draw(|f| ui::draw(f, &app)).unwrap();
        let buf25: String = terminal_small
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(
            buf25.contains("mod-25.jar"),
            "mod-25.jar debe ser visible en pantalla tras scroll"
        );
        assert!(
            !buf25.contains("mod-00.jar"),
            "mod-00.jar debe haberse ocultado por scroll"
        );

        // Al navegar hacia arriba de vuelta a mod-00
        for _ in 0..25 {
            app.update(action::Action::Up, &tx);
        }
        assert_eq!(app.selected_installed_mod, 0);
        terminal_small.draw(|f| ui::draw(f, &app)).unwrap();
        let buf_top: String = terminal_small
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(buf_top.contains("mod-00.jar"));
        assert!(!buf_top.contains("mod-25.jar"));
    }

    #[tokio::test]
    async fn test_mods_tab_add_and_delete_mod_modals() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let paths = SystemPaths::resolve();
        let mut app = App::new(paths);
        let (tx, _rx) = tokio::sync::mpsc::channel(10);

        app.instances.clear();
        app.instances.push(app::InstanceSummary {
            name: "fabric_test".to_string(),
            loader: "fabric".to_string(),
            mc_version: "1.20.1".to_string(),
            ram: "4G".to_string(),
            port: 25565,
            state: app::ServerState::Offline,
            source_pack: None,
        });
        app.selected_instance = 0;
        app.active_tab = app::AppTab::Modrinth;
        app.focus = app::Focus::MainPanel;

        app.installed_mods = vec![app::InstalledMod {
            file_name: "lithium-0.11.2.jar".to_string(),
            size_bytes: 350 * 1024,
        }];

        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();

        // 1. Probar modal de Añadir Mod ('u')
        app.update(action::Action::OpenAddModModal, &tx);
        assert!(app.show_add_mod_modal);
        assert_eq!(app.input_mode, app::InputMode::Insert);

        assert!(terminal.draw(|f| ui::draw(f, &app)).is_ok());
        let add_content: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(add_content.contains("Instalar Mod"));
        assert!(add_content.contains("URL o Slug"));

        app.update(action::Action::CloseAddModModal, &tx);
        assert!(!app.show_add_mod_modal);
        assert_eq!(app.input_mode, app::InputMode::Normal);

        // 2. Probar modal de Borrar Mod ('d')
        app.selected_installed_mod = 0;
        app.update(action::Action::PromptDeleteMod, &tx);
        assert!(app.show_delete_mod_modal);
        assert_eq!(
            app.delete_mod_filename.as_deref(),
            Some("lithium-0.11.2.jar")
        );

        assert!(terminal.draw(|f| ui::draw(f, &app)).is_ok());
        let del_content: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(del_content.contains("Eliminar Mod"));
        assert!(del_content.contains("lithium-0.11.2.jar"));

        app.update(action::Action::CancelDeleteMod, &tx);
        assert!(!app.show_delete_mod_modal);
        assert!(app.delete_mod_filename.is_none());
    }

    #[tokio::test]
    async fn test_create_modal_enter_disambiguation_and_esc_cancel() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let paths = SystemPaths::resolve();
        let mut app = App::new(paths);
        let (tx, _rx) = tokio::sync::mpsc::channel(10);

        // 1. Abrir modal -> Debe iniciar en modo normal de navegación
        app.update(action::Action::OpenCreateModal, &tx);
        assert!(app.show_create_modal);
        assert_eq!(app.input_mode, app::InputMode::Normal);
        assert_eq!(app.create_field_idx, 0);

        // 2. En modo Normal, 'Enter' debe intentar CREAR la instancia (SubmitCreateInstance), NO editar
        let enter_key = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let action = app.handle_key(enter_key);
        assert_eq!(action, Some(action::Action::SubmitCreateInstance));
        assert_eq!(app.input_mode, app::InputMode::Normal);

        // 3. Edición exclusiva con 'i': Inicia modo edición y guarda backup
        let i_key = KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE);
        let action = app.handle_key(i_key);
        assert_eq!(action, None);
        assert_eq!(app.input_mode, app::InputMode::Insert);
        assert!(app.create_input_backup.is_some());

        // Modificar texto del campo
        let char_key = KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE);
        app.handle_key(char_key);
        let char_key2 = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE);
        app.handle_key(char_key2);
        assert_eq!(app.create_name_input.value(), "mc");

        // 4. 'Esc' durante edición debe CANCELAR y restaurar el valor previo
        let esc_key = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        let action = app.handle_key(esc_key);
        assert_eq!(action, None);
        assert_eq!(app.input_mode, app::InputMode::Normal);
        assert_eq!(app.create_name_input.value(), ""); // Restaurado a vacío (backup)

        // 5. Entrar a editar de nuevo, escribir y confirmar con 'Enter'
        app.handle_key(i_key);
        assert_eq!(app.input_mode, app::InputMode::Insert);
        app.handle_key(char_key);
        app.handle_key(char_key2);
        assert_eq!(app.create_name_input.value(), "mc");

        // 'Enter' en modo Insert confirma el texto y regresa a modo Normal del modal
        let action = app.handle_key(enter_key);
        assert_eq!(action, None);
        assert_eq!(app.input_mode, app::InputMode::Normal);
        assert_eq!(app.create_name_input.value(), "mc");

        // 6. 'Esc' en modo normal debe CERRAR el modal
        let action = app.handle_key(esc_key);
        assert_eq!(action, Some(action::Action::CloseCreateModal));

        // 7. Verificar que el footer dibuja las directivas actualizadas
        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        assert!(terminal.draw(|f| ui::draw(f, &app)).is_ok());
        let content: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(content.contains("i: Editar"));
        assert!(content.contains("Enter: Crear"));
        assert!(!content.contains("i/Enter: Editar"));
    }

    #[tokio::test]
    async fn test_addons_subfolder_and_tab3_footer() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let paths = SystemPaths::resolve();
        let mut app = App::new(paths);
        app.instances.clear();
        app.instances.push(app::InstanceSummary {
            name: "paper_srv".to_string(),
            loader: "paper".to_string(),
            mc_version: "1.21".to_string(),
            ram: "4G".to_string(),
            port: 25565,
            state: app::ServerState::Offline,
            source_pack: None,
        });
        app.instances.push(app::InstanceSummary {
            name: "fabric_srv".to_string(),
            loader: "fabric".to_string(),
            mc_version: "1.20.1".to_string(),
            ram: "4G".to_string(),
            port: 25566,
            state: app::ServerState::Offline,
            source_pack: None,
        });

        // Verificar detección automática de carpeta destino
        assert_eq!(app.instances[0].addons_subfolder(), "plugins");
        assert_eq!(app.instances[1].addons_subfolder(), "mods");

        // Verificar footer del Tab 3
        app.selected_instance = 0;
        app.active_tab = app::AppTab::Modrinth;
        app.focus = app::Focus::MainPanel;

        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        assert!(terminal.draw(|f| ui::draw(f, &app)).is_ok());

        let footer_content: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect();

        assert!(
            footer_content.contains("u: Instalar por URL"),
            "Footer debe contener 'u: Instalar por URL'"
        );
        assert!(
            footer_content.contains("d: Eliminar"),
            "Footer debe contener 'd: Eliminar'"
        );
    }
}
