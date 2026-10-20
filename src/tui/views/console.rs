use crate::tui::app::{App, InputMode};
use crate::tui::theme::Theme;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};

pub fn draw(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),    // Visor de logs
            Constraint::Length(3), // Input de comandos
        ])
        .split(area);

    draw_logs_window(f, chunks[0], app, theme);
    draw_command_input(f, chunks[1], app, theme);
}

fn draw_logs_window(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let title = if let Some(inst) = app.selected() {
        let scroll_hint = if app.console_scroll > 0 {
            format!(" [Historial: +{} líneas] ", app.console_scroll)
        } else {
            " [En vivo] ".to_string()
        };
        format!(" Consola: {} {} ", inst.name, scroll_hint)
    } else {
        " Consola ".to_string()
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.border)
        .title(title);

    let inner_height = area.height.saturating_sub(2) as usize;

    let total_lines = app.log_buffer.len();
    let visible_lines: Vec<Line> = if total_lines == 0 {
        vec![Line::from(Span::styled(
            " No hay registros disponibles para esta instancia.",
            theme.muted,
        ))]
    } else {
        let end_idx = total_lines.saturating_sub(app.console_scroll);
        let start_idx = end_idx.saturating_sub(inner_height);

        app.log_buffer
            .range(start_idx..end_idx)
            .map(|raw_line| format_colored_line(raw_line, theme))
            .collect()
    };

    let p = Paragraph::new(visible_lines).block(block);
    f.render_widget(p, area);
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
