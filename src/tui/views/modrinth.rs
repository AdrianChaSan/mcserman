use crate::modrinth::api::format_downloads;
use crate::tui::app::{App, Focus, InputMode};
use crate::tui::theme::Theme;
use ratatui::prelude::*;
use ratatui::widgets::{
    Block, Borders, Gauge, List, ListItem, Paragraph, Scrollbar, ScrollbarOrientation,
    ScrollbarState,
};

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
                "  Para instalar optimizaciones o extensiones de Modrinth, crea una instancia Paper o Fabric.",
                theme.muted,
            )),
        ];
        let p = Paragraph::new(text).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(theme.border)
                .title(" Modrinth "),
        );
        f.render_widget(p, area);
        return;
    }

    if let Some((progress, ref msg)) = app.modrinth_progress {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Input de búsqueda
                Constraint::Min(0),    // Lista de resultados
                Constraint::Length(3), // Barra de descarga
            ])
            .split(area);

        draw_search_bar(
            f,
            chunks[0],
            app,
            inst.loader.as_str(),
            &inst.mc_version,
            theme,
        );
        draw_results_list(f, chunks[1], app, theme);
        draw_progress_bar(f, chunks[2], progress, msg, theme);
    } else {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Input de búsqueda
                Constraint::Min(0),    // Lista de resultados expandida
            ])
            .split(area);

        draw_search_bar(
            f,
            chunks[0],
            app,
            inst.loader.as_str(),
            &inst.mc_version,
            theme,
        );
        draw_results_list(f, chunks[1], app, theme);
    }
}

fn draw_search_bar(
    f: &mut Frame,
    area: Rect,
    app: &App,
    loader: &str,
    mc_version: &str,
    theme: &Theme,
) {
    let is_insert = app.input_mode == InputMode::Insert;
    let is_main_focused = app.focus == Focus::MainPanel;
    let border_style = if is_insert || is_main_focused {
        theme.active_border
    } else {
        theme.border
    };

    let title = format!(" Buscar en Modrinth para {} {} ", loader, mc_version);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(title);

    let val = app.modrinth_input.value();
    let prompt = Span::styled("🔍 ", theme.accent);
    let text = Span::styled(val, theme.base);

    let line = Line::from(vec![prompt, text]);
    let p = Paragraph::new(line).block(block);
    f.render_widget(p, area);

    if is_insert {
        let cursor_x = area.x + 3 + app.modrinth_input.visual_cursor() as u16;
        let cursor_y = area.y + 1;
        if cursor_x < area.x + area.width - 1 {
            f.set_cursor_position((cursor_x, cursor_y));
        }
    }
}

fn draw_results_list(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let is_main_focused = app.focus == Focus::MainPanel;
    let border_style = if is_main_focused {
        theme.active_border
    } else {
        theme.border
    };

    let title = if app.modrinth_hits.is_empty() {
        " Resultados ".to_string()
    } else {
        format!(
            " Resultados ({}/{}) ",
            app.selected_mod_hit + 1,
            app.modrinth_hits.len()
        )
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(title);

    if app.modrinth_loading {
        let p = Paragraph::new(" Consultando Modrinth API...")
            .style(theme.accent)
            .block(block);
        f.render_widget(p, area);
        return;
    }

    if let Some(ref err) = app.modrinth_error {
        let p = Paragraph::new(format!(" ✗ Error: {}", err))
            .style(theme.error)
            .block(block);
        f.render_widget(p, area);
        return;
    }

    if app.modrinth_hits.is_empty() {
        let p = Paragraph::new(" Escribe un término de búsqueda (ej. 'lithium', 'chunky', 'viaversion') y presiona Enter.")
            .style(theme.muted)
            .block(block);
        f.render_widget(p, area);
        return;
    }

    let max_desc_width = (area.width.saturating_sub(6)) as usize;

    let items: Vec<ListItem> = app
        .modrinth_hits
        .iter()
        .enumerate()
        .map(|(i, hit)| {
            let is_selected = i == app.selected_mod_hit;
            let pointer = if is_selected && is_main_focused {
                "▸ "
            } else {
                "  "
            };

            let mut header_spans = vec![
                Span::styled(
                    pointer,
                    if is_selected && is_main_focused {
                        theme.accent
                    } else {
                        theme.muted
                    },
                ),
                Span::styled(
                    &hit.title,
                    if is_selected {
                        if is_main_focused {
                            theme.highlight
                        } else {
                            Style::default()
                                .fg(Color::White)
                                .add_modifier(Modifier::BOLD)
                        }
                    } else {
                        theme.bold
                    },
                ),
            ];

            if let Some(ref author) = hit.author {
                header_spans.push(Span::styled(format!(" por {}", author), theme.muted));
            }

            if let Some(dl) = hit.downloads {
                header_spans.push(Span::styled(
                    format!("  ⬇ {}", format_downloads(dl)),
                    theme.accent,
                ));
            }

            let header = Line::from(header_spans);

            let desc_str = if hit.description.chars().count() > max_desc_width {
                let mut truncated: String = hit
                    .description
                    .chars()
                    .take(max_desc_width.saturating_sub(3))
                    .collect();
                truncated.push_str("...");
                truncated
            } else {
                hit.description.clone()
            };

            let desc = Line::from(vec![
                Span::styled("    ", theme.muted),
                Span::styled(desc_str, theme.muted),
            ]);

            ListItem::new(vec![header, desc])
        })
        .collect();

    let list = List::new(items).block(block);
    f.render_widget(list, area);

    // Scrollbar en el margen derecho de resultados
    if !app.modrinth_hits.is_empty() {
        let mut scrollbar_state =
            ScrollbarState::new(app.modrinth_hits.len()).position(app.selected_mod_hit);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("▲"))
            .end_symbol(Some("▼"))
            .track_symbol(Some("│"))
            .thumb_symbol("█");
        f.render_stateful_widget(scrollbar, area, &mut scrollbar_state);
    }
}

fn draw_progress_bar(f: &mut Frame, area: Rect, progress: f64, msg: &str, theme: &Theme) {
    let ratio = (progress / 100.0).clamp(0.0, 1.0);
    let gauge = Gauge::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(theme.accent)
                .title(" Descarga de Mod/Plugin "),
        )
        .gauge_style(theme.highlight)
        .ratio(ratio)
        .label(msg);
    f.render_widget(gauge, area);
}
