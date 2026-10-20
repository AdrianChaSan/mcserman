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
                    tick_counter = tick_counter.wrapping_add(1);
                    // Actualizar estado de los sockets cada 10 ticks (1 segundo)
                    if tick_counter.is_multiple_of(10) {
                        app.refresh_online_status().await;
                    }
                }
                AppEvent::LogLineReceived { instance, line } => {
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
                AppEvent::InstanceStatusChanged { name, is_online } => {
                    if let Some(inst) = app.instances.iter_mut().find(|i| i.name == name) {
                        inst.is_online = is_online;
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
                    app.create_versions = vers;
                    app.create_loading_versions = false;
                    app.create_version_idx = 0;
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
}
