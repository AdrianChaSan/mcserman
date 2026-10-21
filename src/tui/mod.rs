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

    let mut event_handler = EventHandler::new(256);
    let event_tx = event_handler.sender();
    spawn_event_loop(event_tx.clone(), Duration::from_millis(100));

    // Iniciar streaming de consola si la pestaña inicial lo requiere
    if app.active_tab == app::AppTab::Console {
        app.start_console_stream(&event_tx);
    }

    let mut tick_counter: u32 = 0;

    // Render inicial
    terminal.draw(|f| ui::draw(f, &app))?;

    while !app.should_quit {
        if let Some(event) = event_handler.next().await {
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
                    if let Some(inst) = app.instances.iter_mut().find(|i| i.name == instance) {
                        if inst.state == app::ServerState::Offline {
                            inst.state = app::ServerState::Starting;
                        }
                    }
                    if let Some(selected) = app.selected() {
                        if selected.name == instance {
                            app.push_log_line(line);
                        }
                    }
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
                AppEvent::ModrinthSearchResults(hits) => {
                    app.modrinth_hits = hits;
                    app.modrinth_loading = false;
                    app.selected_mod_hit = 0;
                    app.input_mode = InputMode::Normal;
                }
                AppEvent::ModrinthSearchError(err) => {
                    app.modrinth_error = Some(err);
                    app.modrinth_loading = false;
                    app.modrinth_progress = None;
                }
                AppEvent::ModrinthInstalled(title) => {
                    app.modrinth_progress = None;
                    app.set_status(&format!(
                        "✓ {} instalado correctamente en mods/plugins",
                        title
                    ));
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

            // Redibujar tras procesar evento
            terminal.draw(|f| ui::draw(f, &app))?;
        }
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
            first.contains("500"),
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
    fn test_format_downloads() {
        assert_eq!(crate::modrinth::api::format_downloads(500), "500");
        assert_eq!(crate::modrinth::api::format_downloads(1_500), "1.5K");
        assert_eq!(crate::modrinth::api::format_downloads(1_200_000), "1.2M");
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
}
