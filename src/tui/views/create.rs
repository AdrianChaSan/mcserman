use crate::tui::app::App;
use crate::tui::theme::Theme;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Gauge, Paragraph};

pub fn draw_modal(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let popup = centered_rect(64, 75, area);

    // Limpiar el fondo del modal
    f.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.active_border)
        .title(" 🛠️  Crear Nueva Instancia de Servidor ");

    f.render_widget(block, popup);

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Margen superior
            Constraint::Length(3), // 0. Nombre
            Constraint::Length(3), // 1. Loader
            Constraint::Length(3), // 2. Versión
            Constraint::Length(3), // 3. RAM
            Constraint::Length(3), // 4. Puerto
            Constraint::Length(2), // Error o Progreso
            Constraint::Length(1), // Barra de ayuda
        ])
        .margin(1)
        .split(popup);

    // 0. Nombre
    draw_input_field(
        f,
        inner[1],
        " Nombre identificador ",
        app.create_name_input.value(),
        app.create_field_idx == 0,
        theme,
        app.create_name_input.visual_cursor(),
    );

    // 1. Loader
    draw_loader_selector(
        f,
        inner[2],
        app.create_loader_idx,
        app.create_field_idx == 1,
        theme,
    );

    // 2. Versión
    draw_version_selector(
        f,
        inner[3],
        &app.create_versions,
        app.create_version_idx,
        app.create_loading_versions,
        app.create_field_idx == 2,
        theme,
    );

    // 3. RAM
    draw_input_field(
        f,
        inner[4],
        " Memoria RAM máxima (ej. 2G, 4G, 8G) ",
        app.create_ram_input.value(),
        app.create_field_idx == 3,
        theme,
        app.create_ram_input.visual_cursor(),
    );

    // 4. Puerto
    draw_input_field(
        f,
        inner[5],
        " Puerto TCP del servidor ",
        app.create_port_input.value(),
        app.create_field_idx == 4,
        theme,
        app.create_port_input.visual_cursor(),
    );

    // 5. Progreso o Error
    if let Some((ratio, ref msg)) = app.create_progress {
        let gauge = Gauge::default()
            .gauge_style(theme.highlight)
            .ratio((ratio / 100.0).clamp(0.0, 1.0))
            .label(msg.as_str());
        f.render_widget(gauge, inner[6]);
    } else if let Some(ref err) = app.create_error {
        let p = Paragraph::new(format!(" ✗ {}", err)).style(theme.error);
        f.render_widget(p, inner[6]);
    }

    // 6. Pie de ayuda
    let hints = Line::from(vec![
        Span::styled(" Tab: ", theme.accent),
        Span::styled("Siguiente   ", theme.base),
        Span::styled("Enter: ", theme.accent),
        Span::styled("Crear servidor   ", theme.base),
        Span::styled("Esc: ", theme.accent),
        Span::styled("Cancelar", theme.muted),
    ]);
    f.render_widget(Paragraph::new(hints), inner[7]);
}

fn draw_input_field(
    f: &mut Frame,
    area: Rect,
    label: &str,
    val: &str,
    is_active: bool,
    theme: &Theme,
    cursor: usize,
) {
    let border_style = if is_active {
        theme.active_border
    } else {
        theme.border
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(label);

    let p = Paragraph::new(format!(" {}", val)).block(block);
    f.render_widget(p, area);

    if is_active {
        let cursor_x = area.x + 2 + cursor as u16;
        let cursor_y = area.y + 1;
        if cursor_x < area.x + area.width - 1 {
            f.set_cursor_position((cursor_x, cursor_y));
        }
    }
}

fn draw_loader_selector(
    f: &mut Frame,
    area: Rect,
    selected_idx: usize,
    is_active: bool,
    theme: &Theme,
) {
    let border_style = if is_active {
        theme.active_border
    } else {
        theme.border
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(" Tipo de Servidor (Modloader) - Flechas ◄ ► ");

    let loaders = ["Paper (Recomendado)", "Fabric (Mods)", "Vanilla"];
    let mut spans = vec![Span::raw(" ")];
    for (i, name) in loaders.iter().enumerate() {
        if i == selected_idx {
            spans.push(Span::styled(format!(" [● {}] ", name), theme.accent));
        } else {
            spans.push(Span::styled(format!("  ○ {}  ", name), theme.muted));
        }
    }

    let p = Paragraph::new(Line::from(spans)).block(block);
    f.render_widget(p, area);
}

fn draw_version_selector(
    f: &mut Frame,
    area: Rect,
    versions: &[String],
    selected_idx: usize,
    is_loading: bool,
    is_active: bool,
    theme: &Theme,
) {
    let border_style = if is_active {
        theme.active_border
    } else {
        theme.border
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(" Versión de Minecraft - Flechas ◄ ► ");

    let line = if is_loading {
        Line::from(Span::styled(
            "  Consultando versiones disponibles desde API...",
            theme.accent,
        ))
    } else if versions.is_empty() {
        Line::from(Span::styled("  1.21.4 (Default)", theme.base))
    } else {
        let current_ver = versions
            .get(selected_idx)
            .map(|s| s.as_str())
            .unwrap_or("1.21.4");
        Line::from(vec![
            Span::styled("  ◄ ", theme.accent),
            Span::styled(format!("Minecraft {}", current_ver), theme.bold),
            Span::styled(
                format!("  ({}/{})", selected_idx + 1, versions.len()),
                theme.muted,
            ),
            Span::styled(" ►", theme.accent),
        ])
    };

    let p = Paragraph::new(line).block(block);
    f.render_widget(p, area);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
