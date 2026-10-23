use crate::tui::app::App;
use crate::tui::theme::Theme;
use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap};

pub fn draw_modal(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let server_name = app
        .delete_server_name
        .as_deref()
        .or_else(|| app.selected().map(|i| i.name.as_str()))
        .unwrap_or("servidor");

    let dim_block = Block::default().style(Style::default().bg(Color::Rgb(10, 10, 15)));
    f.render_widget(dim_block, area);

    let popup = centered_rect(78, 8, area);

    f.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(theme.error)
        .style(Style::default().bg(Color::Rgb(18, 18, 24)))
        .title(" ⚠️  Eliminar Servidor ");

    let content_area = block.inner(popup);
    f.render_widget(block, popup);

    let lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("¿Eliminar servidor '", theme.base),
            Span::styled(server_name, theme.bold),
            Span::styled("'? Esta acción borrará sus archivos.", theme.base),
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

    let p = Paragraph::new(lines)
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: false });
    f.render_widget(p, content_area);
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
