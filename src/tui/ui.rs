use crate::tui::app::{App, AppTab, InputMode};
use crate::tui::theme::{
    Theme, HEADER_HEIGHT, MIN_HEIGHT, MIN_WIDTH, SIDEBAR_WIDTH, STATUS_BAR_HEIGHT,
};
use crate::tui::views;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Tabs};

pub fn draw(f: &mut Frame, app: &App) {
    let theme = Theme::new();
    let area = f.area();

    // 1. Verificación de tamaño mínimo de terminal
    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        let msg = format!(
            "Terminal demasiado pequeña ({}x{}). Mínimo requerido: {}x{}",
            area.width, area.height, MIN_WIDTH, MIN_HEIGHT
        );
        let p = Paragraph::new(msg)
            .style(theme.warning)
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(theme.error),
            );
        f.render_widget(p, area);
        return;
    }

    // 2. División Vertical: Header + Contenido + Footer
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(HEADER_HEIGHT),
            Constraint::Min(0),
            Constraint::Length(STATUS_BAR_HEIGHT),
        ])
        .split(area);

    draw_header(f, chunks[0], app, &theme);
    draw_body(f, chunks[1], app, &theme);
    draw_footer(f, chunks[2], app, &theme);

    // 3. Modales (si están abiertos)
    if app.show_create_modal {
        views::create::draw_modal(f, area, app, &theme);
    } else if app.show_delete_modal {
        views::delete::draw_modal(f, area, app, &theme);
    } else if app.show_add_mod_modal {
        views::modrinth::draw_add_mod_modal(f, area, app, &theme);
    } else if app.show_delete_mod_modal {
        views::modrinth::draw_delete_mod_modal(f, area, app, &theme);
    }
}

fn draw_header(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let mode_str = if app.input_mode == InputMode::Insert {
        Span::styled("[MODO INSERT] ", theme.accent)
    } else {
        Span::styled("[MODO NORMAL] ", theme.muted)
    };

    let title_line = Line::from(vec![
        Span::styled(" ⛏️  ", theme.bold),
        Span::styled("mcserman", theme.header),
        Span::styled(" — Gestor de Servidores Minecraft ", theme.base),
        Span::styled("(NixOS/Linux) ", theme.muted),
        mode_str,
    ]);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.border);

    let p = Paragraph::new(title_line).block(block);
    f.render_widget(p, area);
}

fn draw_body(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(SIDEBAR_WIDTH), Constraint::Min(0)])
        .split(area);

    draw_sidebar(f, columns[0], app, theme);
    draw_main_panel(f, columns[1], app, theme);
}

fn draw_sidebar(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let is_sidebar_focused = app.focus == crate::tui::app::Focus::Sidebar;
    let border_style = if is_sidebar_focused {
        theme.active_border
    } else {
        theme.border
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(format!(" 📦 Servidores ({}) ", app.instances.len()));

    if app.instances.is_empty() {
        let empty_msg = Paragraph::new(" Sin instancias\n Presiona [n] para crear")
            .style(theme.muted)
            .block(block);
        f.render_widget(empty_msg, area);
        return;
    }

    let items: Vec<ListItem> =
        app.instances
            .iter()
            .enumerate()
            .map(|(i, inst)| {
                let is_selected = i == app.selected_instance;
                let pointer = if is_selected && is_sidebar_focused {
                    "▸ "
                } else {
                    "  "
                };

                let status_dot = match inst.state {
                    crate::tui::app::ServerState::Online => Span::styled("● ", theme.success),
                    crate::tui::app::ServerState::Starting
                    | crate::tui::app::ServerState::Stopping => Span::styled("◐ ", theme.warning),
                    crate::tui::app::ServerState::Offline => Span::styled("○ ", theme.muted),
                };

                let name_style = if is_selected {
                    if is_sidebar_focused {
                        theme.highlight
                    } else {
                        Style::default().fg(Color::White).bg(Color::Rgb(28, 34, 44))
                    }
                } else {
                    theme.base
                };

                let line = Line::from(vec![
                    Span::styled(
                        pointer,
                        if is_selected && is_sidebar_focused {
                            theme.accent
                        } else {
                            theme.muted
                        },
                    ),
                    status_dot,
                    Span::styled(&inst.name, name_style),
                ]);

                ListItem::new(line)
            })
            .collect();

    let list = List::new(items).block(block);
    let mut list_state = app.instances_list_state.borrow_mut();
    list_state.select(Some(app.selected_instance));
    f.render_stateful_widget(list, area, &mut *list_state);
}

fn draw_main_panel(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    let is_main_focused = app.focus == crate::tui::app::Focus::MainPanel;
    let tab_border = if is_main_focused {
        theme.active_border
    } else {
        theme.border
    };

    // Barra de Tabs
    let tab_titles: Vec<Line> = AppTab::ALL
        .iter()
        .map(|t| {
            let style = if *t == app.active_tab {
                theme.accent
            } else {
                theme.muted
            };
            Line::from(Span::styled(t.title(), style))
        })
        .collect();

    let tabs = Tabs::new(tab_titles)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(tab_border),
        )
        .select(app.active_tab as usize)
        .highlight_style(theme.accent);

    f.render_widget(tabs, chunks[0]);

    // Contenido de la pestaña activa delegada a submódulos de views/
    match app.active_tab {
        AppTab::Dashboard => views::dashboard::draw(f, chunks[1], app, theme),
        AppTab::Console => views::console::draw(f, chunks[1], app, theme),
        AppTab::Modrinth => views::modrinth::draw(f, chunks[1], app, theme),
        AppTab::Settings => views::properties::draw(f, chunks[1], app, theme),
    }
}

fn draw_footer(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let footer_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(18)])
        .split(area);

    let base_hints = if app.properties_search_active {
        Span::styled(
            "Enter: Fijar búsqueda │ Esc: Cancelar búsqueda",
            theme.accent,
        )
    } else if app.show_delete_modal || app.show_delete_mod_modal {
        Span::styled(
            "y/Enter: Confirmar eliminación │ n/Esc: Cancelar",
            theme.warning,
        )
    } else if app.show_add_mod_modal {
        Span::styled(
            "Enter: Descargar e instalar extensión │ Esc: Cancelar",
            theme.accent,
        )
    } else if app.show_create_modal {
        if app.input_mode == InputMode::Insert {
            Span::styled(
                "Esc: Cancelar │ Tab: Siguiente │ Enter: Confirmar",
                theme.accent,
            )
        } else {
            Span::styled(
                "Esc: Cerrar │ Tab/↓: Siguiente │ ◄►: Cambiar │ i: Editar │ Enter: Crear",
                theme.base,
            )
        }
    } else if app.input_mode == InputMode::Insert {
        if app.editing_property.is_some() {
            Span::styled("Esc: Cancelar edición │ Enter: Guardar valor", theme.accent)
        } else {
            Span::styled(
                "Esc: Salir a Modo Normal │ Enter: Confirmar/Enviar",
                theme.accent,
            )
        }
    } else {
        match app.active_tab {
            AppTab::Dashboard => {
                if app.focus == crate::tui::app::Focus::Sidebar {
                    Span::styled(
                        "q: Salir │ j/k: Servidor │ Space: Iniciar/Parar │ n: Nuevo │ d: Eliminar │ l/→: Panel │ 1-4: Tabs",
                        theme.base,
                    )
                } else {
                    Span::styled(
                        "q: Salir │ h/←: Sidebar │ Space: Iniciar/Parar │ n: Nuevo │ d: Eliminar │ 1-4: Tabs",
                        theme.base,
                    )
                }
            }
            AppTab::Console => {
                if app.focus == crate::tui::app::Focus::Sidebar {
                    Span::styled(
                        "q: Salir │ j/k: Servidor │ Space: Iniciar/Parar │ n: Nuevo │ d: Eliminar │ l/→: Consola │ 1-4: Tabs",
                        theme.base,
                    )
                } else {
                    Span::styled(
                        "q: Salir │ i/Enter: Escribir comando │ PgUp/PgDn: Historial │ h/l: Foco │ 1-4: Tabs",
                        theme.base,
                    )
                }
            }
            AppTab::Modrinth => {
                if app.focus == crate::tui::app::Focus::Sidebar {
                    Span::styled(
                        "q: Salir │ j/k: Servidor │ Space: Iniciar/Parar │ n: Nuevo │ d: Eliminar │ l/→: Mods │ 1-4: Tabs",
                        theme.base,
                    )
                } else {
                    Span::styled(
                        "q: Salir │ j/k: Navegar │ u: Instalar por URL │ d: Eliminar │ 1-4: Tabs",
                        theme.base,
                    )
                }
            }
            AppTab::Settings => {
                if app.focus == crate::tui::app::Focus::Sidebar {
                    Span::styled(
                        "q: Salir │ j/k: Servidor │ Space: Iniciar/Parar │ n: Nuevo │ d: Eliminar │ l/→: Opciones │ 1-4: Tabs",
                        theme.base,
                    )
                } else {
                    Span::styled(
                        "q: Salir │ j/k: Navegar │ a: Todas/Comunes │ /: Buscar │ Space: Alternar │ Enter: Editar │ s: Guardar",
                        theme.base,
                    )
                }
            }
        }
    };

    // Auto-expiración del toast/mensaje a los 3 segundos
    let status_active = if let Some((ref msg, instant)) = app.status_message {
        if instant.elapsed() <= std::time::Duration::from_secs(3) {
            Some(msg)
        } else {
            None
        }
    } else {
        None
    };

    let line_spans = if let Some(msg) = status_active {
        vec![
            Span::styled(format!(" ℹ️ {} ", msg), theme.accent),
            Span::styled("│ ", theme.muted),
            base_hints,
        ]
    } else {
        vec![Span::raw(" "), base_hints]
    };

    let left_p =
        Paragraph::new(Line::from(line_spans)).style(Style::default().bg(Color::Rgb(24, 28, 36)));
    f.render_widget(left_p, footer_chunks[0]);

    let version_p = Paragraph::new(concat!("mcserman v", env!("CARGO_PKG_VERSION"), " "))
        .style(theme.muted.bg(Color::Rgb(24, 28, 36)))
        .alignment(Alignment::Right);
    f.render_widget(version_p, footer_chunks[1]);
}
