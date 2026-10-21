use crate::tui::app::{is_boolean_property, is_enum_property, App, Focus};
use crate::tui::theme::Theme;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

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

    let dirty_indicator = if app.properties_dirty {
        " [* Modificado - 's' para guardar] "
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

    let border_style = if is_main_focused {
        theme.active_border
    } else if app.properties_dirty {
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
        f.render_widget(p, area);
    } else {
        let items: Vec<ListItem> = app
            .properties_list
            .iter()
            .enumerate()
            .map(|(i, (k, v))| {
                let is_selected = i == app.selected_property;
                let pointer = if is_selected && is_main_focused {
                    "▸ "
                } else {
                    "  "
                };

                let name_style = if is_selected {
                    if is_main_focused {
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
                        if is_selected && is_main_focused {
                            theme.accent
                        } else {
                            theme.muted
                        },
                    ),
                    Span::styled(format!("{:<22}", k), name_style),
                    Span::styled(" = ", theme.muted),
                ];

                if let (true, Some(input)) = (is_selected, app.editing_property.as_ref()) {
                    line_spans.push(Span::styled("[✎ ", theme.accent));
                    line_spans.push(Span::styled(
                        input.value(),
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::UNDERLINED),
                    ));
                    line_spans.push(Span::styled(" █]", theme.accent));
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

        let list = List::new(items).block(block);
        f.render_widget(list, area);
    }
}
