use crate::tui::app::{App, InputMode};
use crate::tui::theme::Theme;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Gauge, List, ListItem, Paragraph};

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
            Line::from(Span::styled("  ⚠️  Instancia oficial Vanilla seleccionada", theme.warning)),
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

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Input de búsqueda
            Constraint::Min(0),    // Lista de resultados
            Constraint::Length(3), // Barra de descarga o estado
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
    draw_status_bar(f, chunks[2], app, theme);
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
    let border_style = if is_insert {
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
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.border)
        .title(" Resultados ");

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

    let items: Vec<ListItem> = app
        .modrinth_hits
        .iter()
        .enumerate()
        .map(|(i, hit)| {
            let is_selected = i == app.selected_mod_hit;
            let pointer = if is_selected { "▸ " } else { "  " };

            let header = Line::from(vec![
                Span::styled(
                    pointer,
                    if is_selected {
                        theme.accent
                    } else {
                        theme.muted
                    },
                ),
                Span::styled(
                    &hit.title,
                    if is_selected {
                        theme.highlight
                    } else {
                        theme.bold
                    },
                ),
                Span::styled(format!(" ({})", hit.slug), theme.muted),
            ]);

            let desc = Line::from(vec![
                Span::styled("    ", theme.muted),
                Span::styled(&hit.description, theme.muted),
            ]);

            ListItem::new(vec![header, desc])
        })
        .collect();

    let list = List::new(items).block(block);
    f.render_widget(list, area);
}

fn draw_status_bar(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    if let Some((progress, ref msg)) = app.modrinth_progress {
        let ratio = (progress / 100.0).clamp(0.0, 1.0);
        let gauge = Gauge::default()
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(theme.accent),
            )
            .gauge_style(theme.highlight)
            .ratio(ratio)
            .label(msg.as_str());
        f.render_widget(gauge, area);
    } else {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(theme.border);
        let hints = Line::from(vec![
            Span::styled(" Atajos: ", theme.muted),
            Span::styled("[i/Enter] ", theme.accent),
            Span::styled("Escribir búsqueda   ", theme.base),
            Span::styled("[j/k] ", theme.accent),
            Span::styled("Navegar resultados   ", theme.base),
            Span::styled("[Enter en Normal] ", theme.accent),
            Span::styled("Descargar e Instalar", theme.base),
        ]);
        let p = Paragraph::new(hints).block(block);
        f.render_widget(p, area);
    }
}
