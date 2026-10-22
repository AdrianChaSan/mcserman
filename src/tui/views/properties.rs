use crate::tui::app::{is_boolean_property, is_enum_property, App, Focus};
use crate::tui::theme::Theme;
use ratatui::prelude::*;
use ratatui::widgets::{
    Block, Borders, List, ListItem, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
};

pub fn parse_minecraft_formatting(raw: &str) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut current_color = Color::White;
    let mut current_modifier = Modifier::empty();
    let mut buffer = String::new();

    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '§' {
            if let Some(&code) = chars.peek() {
                chars.next();
                if !buffer.is_empty() {
                    spans.push(Span::styled(
                        std::mem::take(&mut buffer),
                        Style::default()
                            .fg(current_color)
                            .add_modifier(current_modifier),
                    ));
                }
                match code.to_ascii_lowercase() {
                    '0' => current_color = Color::Black,
                    '1' => current_color = Color::Blue,
                    '2' => current_color = Color::Green,
                    '3' => current_color = Color::Cyan,
                    '4' => current_color = Color::Red,
                    '5' => current_color = Color::Magenta,
                    '6' => current_color = Color::Yellow,
                    '7' => current_color = Color::Gray,
                    '8' => current_color = Color::DarkGray,
                    '9' => current_color = Color::LightBlue,
                    'a' => current_color = Color::LightGreen,
                    'b' => current_color = Color::LightCyan,
                    'c' => current_color = Color::LightRed,
                    'd' => current_color = Color::LightMagenta,
                    'e' => current_color = Color::LightYellow,
                    'f' => current_color = Color::White,
                    'l' => current_modifier.insert(Modifier::BOLD),
                    'n' => current_modifier.insert(Modifier::UNDERLINED),
                    'o' => current_modifier.insert(Modifier::ITALIC),
                    'r' => {
                        current_color = Color::White;
                        current_modifier = Modifier::empty();
                    }
                    _ => {}
                }
                continue;
            }
        }
        buffer.push(c);
    }

    if !buffer.is_empty() {
        spans.push(Span::styled(
            buffer,
            Style::default()
                .fg(current_color)
                .add_modifier(current_modifier),
        ));
    }

    if spans.is_empty() {
        spans.push(Span::raw(""));
    }

    spans
}

pub fn draw(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let is_main_focused = app.focus == Focus::MainPanel;
    let is_searching = app.properties_search_active;
    let search_has_query = !app.properties_search_input.value().is_empty();
    let filtered = app.filtered_properties();

    // 1. Subdivisión vertical si el buscador está activo o tiene texto de filtro
    let (search_area_opt, list_area) = if is_searching || search_has_query {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(0)])
            .split(area);
        (Some(chunks[0]), chunks[1])
    } else {
        (None, area)
    };

    // Renderizado del input de búsqueda (si aplica)
    if let Some(search_rect) = search_area_opt {
        let search_block = Block::default()
            .borders(Borders::ALL)
            .border_style(if is_searching {
                theme.active_border
            } else {
                theme.border
            })
            .title(if is_searching {
                " 🔍 Buscar Propiedad "
            } else {
                " 🔍 Filtro Activo "
            });

        let search_line = if is_searching {
            Line::from(vec![
                Span::raw(" "),
                Span::styled(
                    app.properties_search_input.value(),
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("█", theme.accent),
                Span::styled("  (Enter: Fijar │ Esc: Cancelar)", theme.muted),
            ])
        } else {
            Line::from(vec![
                Span::raw(" "),
                Span::styled(
                    app.properties_search_input.value(),
                    Style::default().fg(Color::Yellow),
                ),
                Span::styled("  (/: Buscar │ Esc: Limpiar)", theme.muted),
            ])
        };

        let search_p = Paragraph::new(search_line).block(search_block);
        f.render_widget(search_p, search_rect);
    }

    // 2. Lista de Propiedades (100% de ancho)
    let dirty_indicator = if app.properties_dirty {
        " [* Modificado - 's' guardar]"
    } else {
        ""
    };

    let mode_desc = if app.properties_show_all {
        "[Todas] (Presiona 'a' para filtrar comunes)"
    } else {
        "[Comunes] (Presiona 'a' para ver todas)"
    };

    let list_title = if let Some(inst) = app.selected() {
        if search_has_query || is_searching {
            format!(
                " Configuración: {} [Búsqueda: \"{}\"] ({}){} ",
                inst.name,
                app.properties_search_input.value(),
                filtered.len(),
                dirty_indicator
            )
        } else {
            format!(
                " Configuración: {} {} ({}){} ",
                inst.name,
                mode_desc,
                filtered.len(),
                dirty_indicator
            )
        }
    } else {
        format!(" Configuración {} ", mode_desc)
    };

    let border_style = if is_main_focused && !is_searching {
        theme.active_border
    } else if app.properties_dirty {
        theme.warning
    } else {
        theme.border
    };

    let list_block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(list_title);

    if filtered.is_empty() {
        let msg = if search_has_query {
            " No se encontraron propiedades que coincidan con la búsqueda.\n Presiona [Esc] para limpiar el filtro."
        } else {
            " No se encontraron propiedades configuradas para este servidor."
        };
        let p = Paragraph::new(msg).style(theme.muted).block(list_block);
        f.render_widget(p, list_area);
    } else {
        let items: Vec<ListItem> = filtered
            .iter()
            .enumerate()
            .map(|(filter_idx, &(_orig_idx, (k, v)))| {
                let is_selected = filter_idx == app.selected_property;
                let pointer = if is_selected && is_main_focused && !is_searching {
                    "▸ "
                } else {
                    "  "
                };

                let name_style = if is_selected {
                    if is_main_focused && !is_searching {
                        theme.highlight
                    } else {
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::BOLD)
                    }
                } else {
                    theme.bold
                };

                let mut line_spans = vec![
                    Span::styled(
                        pointer,
                        if is_selected && is_main_focused && !is_searching {
                            theme.accent
                        } else {
                            theme.muted
                        },
                    ),
                    Span::styled(format!("{:<26}", k), name_style),
                    Span::styled(" = ", theme.muted),
                ];

                if is_selected && app.editing_property.is_some() {
                    if let Some(input) = app.editing_property.as_ref() {
                        line_spans.push(Span::styled("[✎ ", theme.accent));
                        line_spans.push(Span::styled(
                            input.value(),
                            Style::default()
                                .fg(Color::Yellow)
                                .add_modifier(Modifier::UNDERLINED),
                        ));
                        line_spans.push(Span::styled(" █]", theme.accent));
                    }
                } else if is_boolean_property(k, v) {
                    if v == "true" {
                        line_spans.push(Span::styled(
                            "[● true]",
                            Style::default()
                                .fg(Color::Green)
                                .add_modifier(Modifier::BOLD),
                        ));
                    } else {
                        line_spans.push(Span::styled(
                            "[○ false]",
                            Style::default().fg(Color::LightRed),
                        ));
                    }
                } else if is_enum_property(k) {
                    line_spans.push(Span::styled(
                        format!("‹ {} ›", v),
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ));
                } else if k == "motd" {
                    line_spans.extend(parse_minecraft_formatting(v));
                    line_spans.push(Span::styled("  ✎", theme.muted));
                } else {
                    line_spans.push(Span::styled(v.as_str(), theme.base));
                    line_spans.push(Span::styled("  ✎", theme.muted));
                }

                ListItem::new(Line::from(line_spans))
            })
            .collect();

        let list = List::new(items).block(list_block);
        let mut list_state = app.properties_list_state.borrow_mut();
        list_state.select(Some(app.selected_property));
        f.render_stateful_widget(list, list_area, &mut *list_state);

        // Scrollbar vertical en el margen derecho de la lista
        let mut scrollbar_state =
            ScrollbarState::new(filtered.len()).position(app.selected_property);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("▲"))
            .end_symbol(Some("▼"))
            .track_symbol(Some("│"))
            .thumb_symbol("█");
        f.render_stateful_widget(scrollbar, list_area, &mut scrollbar_state);
    }
}
