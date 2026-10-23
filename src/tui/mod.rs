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

    if app.active_tab == app::AppTab::Console {
        app.start_console_stream();
    }

    let mut tick_counter: u32 = 0;

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
mod tests;
