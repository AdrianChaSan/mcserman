use crate::tui::app::{App, Focus, InputMode};
use crate::tui::theme::Theme;
use ratatui::prelude::*;
use ratatui::widgets::{
    Block, BorderType, Borders, Clear, List, ListItem, Paragraph, Scrollbar, ScrollbarOrientation,
    ScrollbarState, Wrap,
};

pub fn format_file_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

pub fn draw(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let inst = match app.selected() {
        Some(i) => i,
        None => {
            let p = Paragraph::new(" No hay ningún servidor seleccionado.")
                .style(theme.muted)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(theme.border),
                );
            f.render_widget(p, area);
            return;
        }
    };

    if inst.loader == "vanilla" {
        let text = vec![
            Line::from(""),
            Line::from(Span::styled(
                "  ⚠️  Instancia oficial Vanilla seleccionada",
                theme.warning,
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  Los servidores Vanilla de Mojang no admiten mods ni plugins de terceros.",
                theme.base,
            )),
            Line::from(Span::styled(
                "  Para instalar optimizaciones o extensiones, crea una instancia Paper o Fabric.",
                theme.muted,
            )),
        ];
        let p = Paragraph::new(text).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(theme.border)
                .title(" Mods / Plugins "),
        );
        f.render_widget(p, area);
        return;
    }

    let is_serverpack = app.is_selected_serverpack();

    let chunks = if is_serverpack {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Banner de Advertencia de Serverpack
                Constraint::Min(0),    // Panel Principal de Mods/Plugins Instalados
            ])
            .split(area)
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0)])
            .split(area)
    };

    let content_area = if is_serverpack {
        draw_serverpack_warning(f, chunks[0]);
        chunks[1]
    } else {
        chunks[0]
    };

    draw_installed_mods(f, content_area, app, theme);
}

fn draw_serverpack_warning(f: &mut Frame, area: Rect) {
    let lines = vec![Line::from(vec![
        Span::styled(
            " ⚠ Advertencia: ",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "Esta instancia fue creada con un Serverpack oficial. Añadir o alterar mods externos puede causar desincronización con los clientes, corromper el mundo o romper la experiencia.",
            Style::default().fg(Color::Yellow),
        ),
    ])];
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow))
        .style(Style::default().bg(Color::Rgb(24, 20, 10)));
    let p = Paragraph::new(lines).block(block).wrap(Wrap { trim: true });
    f.render_widget(p, area);
}

fn draw_installed_mods(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    f.render_widget(Clear, area);
    let is_main_focused = app.focus == Focus::MainPanel;
    let border_style = if is_main_focused {
        theme.active_border
    } else {
        theme.border
    };

    let title = format!(" Mods Instalados ({}) ", app.installed_mods.len());

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(title);

    if app.installed_mods.is_empty() {
        let p = Paragraph::new(
            "\n  No hay mods instalados en esta instancia.\n  Presiona 'u' para instalar vía URL (CurseForge, Modrinth o link directo .jar).",
        )
        .style(theme.muted)
        .block(block);
        f.render_widget(p, area);
        return;
    }

    let inner_width = area.width.saturating_sub(4) as usize;

    let items: Vec<ListItem> = app
        .installed_mods
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let is_selected = i == app.selected_installed_mod;
            let pointer = if is_selected && is_main_focused {
                "▸ "
            } else {
                "  "
            };

            let size_str = format_file_size(m.size_bytes);
            let name_max_width = inner_width.saturating_sub(size_str.len() + 6);
            let mut display_name = m.file_name.clone();
            if display_name.chars().count() > name_max_width {
                let mut truncated: String = display_name
                    .chars()
                    .take(name_max_width.saturating_sub(3))
                    .collect();
                truncated.push_str("...");
                display_name = truncated;
            }

            let name_len = display_name.chars().count();
            let padding_len = inner_width.saturating_sub(name_len + size_str.len() + 4);
            let padding = " ".repeat(padding_len);

            let line = Line::from(vec![
                Span::styled(
                    pointer,
                    if is_selected && is_main_focused {
                        theme.accent
                    } else {
                        theme.muted
                    },
                ),
                Span::styled(
                    "[●] ",
                    if is_selected {
                        theme.accent
                    } else {
                        theme.muted
                    },
                ),
                Span::styled(
                    display_name,
                    if is_selected {
                        if is_main_focused {
                            theme.highlight
                        } else {
                            Style::default()
                                .fg(Color::White)
                                .add_modifier(Modifier::BOLD)
                        }
                    } else {
                        theme.base
                    },
                ),
                Span::raw(padding),
                Span::styled(
                    size_str,
                    if is_selected {
                        theme.accent
                    } else {
                        theme.muted
                    },
                ),
            ]);

            ListItem::new(line)
        })
        .collect();

    let list = List::new(items).block(block);
    let mut list_state = app.installed_mods_state.borrow_mut();
    list_state.select(Some(app.selected_installed_mod));
    f.render_stateful_widget(list, area, &mut *list_state);

    // Scrollbar en el margen derecho sincronizado con la lista
    let visible_height = area.height.saturating_sub(2) as usize;
    if app.installed_mods.len() > visible_height && visible_height > 0 {
        let mut scrollbar_state =
            ScrollbarState::new(app.installed_mods.len()).position(app.selected_installed_mod);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("▲"))
            .end_symbol(Some("▼"))
            .track_symbol(Some("│"))
            .thumb_symbol("█");
        f.render_stateful_widget(scrollbar, area, &mut scrollbar_state);
    }
}

pub fn draw_add_mod_modal(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let dim_block = Block::default().style(Style::default().bg(Color::Rgb(10, 10, 15)));
    f.render_widget(dim_block, area);

    let popup = centered_rect(76, 9, area);
    f.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(theme.active_border)
        .style(Style::default().bg(Color::Rgb(18, 18, 24)))
        .title(" 📥 Instalar Mod (URL o Slug) ");

    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Prompt label
            Constraint::Length(3), // Input box
            Constraint::Length(1), // Error or status
            Constraint::Length(1), // Footer hints
        ])
        .split(inner);

    let label = Paragraph::new(" Instalar Mod (URL o Slug): ").style(theme.base);
    f.render_widget(label, chunks[0]);

    let input_block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.accent)
        .style(Style::default().bg(Color::Rgb(24, 24, 32)));

    let val = app.add_mod_input.value();
    let display_text = if val.is_empty() {
        Span::styled(" ej. URL directa .jar, Modrinth o CurseForge", theme.muted)
    } else {
        Span::styled(val, theme.base)
    };

    let p = Paragraph::new(Line::from(vec![Span::raw(" "), display_text])).block(input_block);
    f.render_widget(p, chunks[1]);

    if app.input_mode == InputMode::Insert {
        let cursor_x = chunks[1].x + 2 + app.add_mod_input.visual_cursor() as u16;
        let cursor_y = chunks[1].y + 1;
        if cursor_x < chunks[1].x + chunks[1].width - 1 {
            f.set_cursor_position((cursor_x, cursor_y));
        }
    }

    if app.add_mod_loading {
        let msg = if let Some((pct, ref info)) = app.modrinth_progress {
            format!(" ⏳ Descargando: {:.1}% ({})", pct, info)
        } else {
            " ⏳ Descargando e instalando extensión...".to_string()
        };
        let status = Paragraph::new(msg).style(theme.accent);
        f.render_widget(status, chunks[2]);
    } else if let Some(ref err) = app.add_mod_error {
        let status = Paragraph::new(format!(" ✗ Error: {}", err)).style(theme.error);
        f.render_widget(status, chunks[2]);
    }

    let hints = Paragraph::new(" [ Enter ] Descargar e Instalar    [ Esc ] Cancelar ")
        .style(theme.muted)
        .alignment(Alignment::Center);
    f.render_widget(hints, chunks[3]);
}

pub fn draw_delete_mod_modal(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let filename = app.delete_mod_filename.as_deref().unwrap_or("archivo.jar");

    let dim_block = Block::default().style(Style::default().bg(Color::Rgb(10, 10, 15)));
    f.render_widget(dim_block, area);

    let popup = centered_rect(72, 7, area);
    f.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(theme.error)
        .style(Style::default().bg(Color::Rgb(18, 18, 24)))
        .title(" ⚠️  Eliminar Mod ");

    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("¿Eliminar '", theme.base),
            Span::styled(filename, theme.bold),
            Span::styled("'? Esta acción borrará el archivo .jar.", theme.base),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("[ y / Enter ] ", theme.error),
            Span::styled("Confirmar", theme.bold),
            Span::styled("   ", theme.base),
            Span::styled("[ n / Esc ] ", theme.muted),
            Span::styled("Cancelar", theme.base),
        ]),
    ];

    let p = Paragraph::new(lines).alignment(Alignment::Center);
    f.render_widget(p, inner);
}

fn centered_rect(width: u16, height: u16, r: Rect) -> Rect {
    let actual_width = width.min(r.width.saturating_sub(2));
    let actual_height = height.min(r.height.saturating_sub(2));

    let v_margin = (r.height.saturating_sub(actual_height)) / 2;
    let h_margin = (r.width.saturating_sub(actual_width)) / 2;

    Rect {
        x: r.x + h_margin,
        y: r.y + v_margin,
        width: actual_width,
        height: actual_height,
    }
}
