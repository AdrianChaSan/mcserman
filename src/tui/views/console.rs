use crate::tui::app::{App, Focus, InputMode, ServerState};
use crate::tui::theme::Theme;
use ratatui::prelude::*;
use ratatui::widgets::{
    Block, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap,
};

pub fn draw(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    f.render_widget(Clear, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(area);

    draw_logs_window(f, chunks[0], app, theme);
    draw_command_input(f, chunks[1], app, theme);
}

fn draw_logs_window(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    f.render_widget(Clear, area);

    let inner_width = area.width.saturating_sub(2) as usize;
    let viewport_height = area.height.saturating_sub(2) as usize;

    let total_visual_lines: usize = if inner_width == 0 {
        0
    } else {
        app.log_buffer
            .iter()
            .map(|raw_line| {
                let line_len = raw_line.chars().count();
                line_len.max(1).div_ceil(inner_width)
            })
            .sum()
    };

    let max_scroll = total_visual_lines.saturating_sub(viewport_height);
    let clamped_scroll = app.console_scroll.min(max_scroll);
    let scroll_y = max_scroll.saturating_sub(clamped_scroll);

    let title = if let Some(inst) = app.selected() {
        let status_hint = match inst.state {
            ServerState::Online => "[● En vivo]".to_string(),
            ServerState::Starting => "[◐ Iniciando...]".to_string(),
            ServerState::Stopping => "[◐ Deteniendo...]".to_string(),
            ServerState::Offline => "[○ Offline]".to_string(),
        };
        let scroll_hint = if clamped_scroll > 0 {
            format!(" [Historial: +{} líneas] ", clamped_scroll)
        } else {
            format!(" {} ", status_hint)
        };
        format!(" Consola: {} {} ", inst.name, scroll_hint)
    } else {
        " Consola ".to_string()
    };

    let border_style = if app.focus == Focus::MainPanel {
        theme.active_border
    } else {
        theme.border
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(title);

    if app.log_buffer.is_empty() {
        let empty_line = Line::from(Span::styled(
            " No hay registros disponibles para esta instancia.",
            theme.muted,
        ));
        let p = Paragraph::new(vec![empty_line]).block(block);
        f.render_widget(p, area);
    } else {
        let total_lines = app.log_buffer.len();

        let mut skip_rows = clamped_scroll;
        let mut end_idx = total_lines;
        if inner_width > 0 {
            while end_idx > 0 && skip_rows > 0 {
                let line_len = app.log_buffer[end_idx - 1].chars().count();
                let visual_rows = line_len.max(1).div_ceil(inner_width);
                if skip_rows >= visual_rows {
                    skip_rows -= visual_rows;
                    end_idx -= 1;
                } else {
                    break;
                }
            }
        }

        let needed_rows = viewport_height + skip_rows;
        let mut accumulated_rows = 0;
        let mut start_idx = end_idx;
        if inner_width > 0 {
            while start_idx > 0 && accumulated_rows < needed_rows {
                start_idx -= 1;
                let line_len = app.log_buffer[start_idx].chars().count();
                let visual_rows = line_len.max(1).div_ceil(inner_width);
                accumulated_rows += visual_rows;
            }
        } else {
            start_idx = end_idx.saturating_sub(viewport_height);
        }

        let slice_lines: Vec<Line> = if start_idx < end_idx {
            app.log_buffer
                .range(start_idx..end_idx)
                .map(|raw_line| format_colored_line(raw_line, theme))
                .collect()
        } else {
            Vec::new()
        };

        let local_scroll_y = accumulated_rows.saturating_sub(viewport_height + skip_rows);

        let p = Paragraph::new(slice_lines)
            .block(block)
            .wrap(Wrap { trim: false })
            .scroll(((local_scroll_y as u16), 0));
        f.render_widget(p, area);
    }

    if max_scroll > 0 {
        let mut scrollbar_state = ScrollbarState::new(max_scroll).position(scroll_y);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("▲"))
            .end_symbol(Some("▼"))
            .track_symbol(Some("│"))
            .thumb_symbol("█");
        f.render_stateful_widget(scrollbar, area, &mut scrollbar_state);
    }
}

fn draw_command_input(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let is_insert = app.input_mode == InputMode::Insert;
    let mode_badge = if is_insert {
        Span::styled(" [INSERT - Escribiendo] ", theme.accent)
    } else {
        Span::styled(
            " [NORMAL - Presiona 'i' o 'Enter' para escribir] ",
            theme.muted,
        )
    };

    let border_style = if is_insert {
        theme.active_border
    } else {
        theme.border
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(mode_badge);

    let input_value = app.console_input.value();
    let prompt_span = Span::styled("> ", theme.accent);
    let text_span = Span::styled(input_value, theme.base);

    let line = Line::from(vec![prompt_span, text_span]);
    let p = Paragraph::new(line).block(block);
    f.render_widget(p, area);

    // Si está en modo insert, posicionar el cursor de la terminal
    if is_insert {
        let cursor_x = area.x + 3 + app.console_input.visual_cursor() as u16;
        let cursor_y = area.y + 1;
        if cursor_x < area.x + area.width - 1 {
            f.set_cursor_position((cursor_x, cursor_y));
        }
    }
}

fn format_colored_line<'a>(line: &'a str, theme: &Theme) -> Line<'a> {
    if line.contains("ERROR") || line.contains("Exception") || line.contains("[STDERR]") {
        Line::from(Span::styled(line, theme.error))
    } else if line.contains("WARN") {
        Line::from(Span::styled(line, theme.warning))
    } else if line.contains("INFO") {
        Line::from(Span::styled(line, theme.base))
    } else {
        Line::from(Span::styled(line, theme.muted))
    }
}
