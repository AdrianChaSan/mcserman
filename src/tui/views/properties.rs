use crate::tui::app::App;
use crate::tui::theme::Theme;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

pub fn draw(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),    // Lista de propiedades
            Constraint::Length(3), // Barra de acciones / guardado
        ])
        .split(area);

    let dirty_indicator = if app.properties_dirty {
        " [* Modificado - Presiona 's' para guardar] "
    } else {
        " "
    };

    let title = if let Some(inst) = app.selected() {
        format!(
            " Configuración: {} (server.properties){} ",
            inst.name, dirty_indicator
        )
    } else {
        " Configuración ".to_string()
    };

    let border_style = if app.properties_dirty {
        theme.warning
    } else {
        theme.border
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(title);

    if app.properties_list.is_empty() {
        let p = Paragraph::new(" No se encontraron propiedades configuradas para este servidor.")
            .style(theme.muted)
            .block(block);
        f.render_widget(p, chunks[0]);
    } else {
        let items: Vec<ListItem> = app
            .properties_list
            .iter()
            .enumerate()
            .map(|(i, (k, v))| {
                let is_selected = i == app.selected_property;
                let pointer = if is_selected { "▸ " } else { "  " };

                let val_style = match v.as_str() {
                    "true" => theme.success,
                    "false" => theme.error,
                    _ => theme.base,
                };

                let line = Line::from(vec![
                    Span::styled(
                        pointer,
                        if is_selected {
                            theme.accent
                        } else {
                            theme.muted
                        },
                    ),
                    Span::styled(
                        format!("{:<20}", k),
                        if is_selected {
                            theme.highlight
                        } else {
                            theme.bold
                        },
                    ),
                    Span::styled(" = ", theme.muted),
                    Span::styled(v, val_style),
                ]);

                ListItem::new(line)
            })
            .collect();

        let list = List::new(items).block(block);
        f.render_widget(list, chunks[0]);
    }

    // Barra de acciones
    let footer_block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.border);

    let hints = Line::from(vec![
        Span::styled(" Atajos: ", theme.muted),
        Span::styled("[j/k] ", theme.accent),
        Span::styled("Navegar   ", theme.base),
        Span::styled("[Space/Enter] ", theme.accent),
        Span::styled("Alternar valor   ", theme.base),
        Span::styled("[s] ", theme.accent),
        Span::styled("Guardar archivo   ", theme.base),
        Span::styled("[r] ", theme.accent),
        Span::styled("Recargar", theme.base),
    ]);

    let p = Paragraph::new(hints).block(footer_block);
    f.render_widget(p, chunks[1]);
}
