use crate::tui::app::{App, InputMode};
use crate::tui::theme::Theme;
use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Clear, Gauge, Padding, Paragraph};

pub fn draw_modal(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    // 1. Fondo de atenuación (dimming) sobre la pantalla completa
    let dim_block = Block::default().style(Style::default().bg(Color::Rgb(10, 10, 15)));
    f.render_widget(dim_block, area);

    // 2. Rectángulo centrado para el modal
    let popup = centered_rect(68, 80, area);

    // 3. Limpiar fondo y renderizar bloque modal opaco con doble borde y padding
    f.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(theme.active_border)
        .style(Style::default().bg(Color::Rgb(18, 18, 24)))
        .title(" 🛠️  Crear Nueva Instancia de Servidor ")
        .padding(Padding::uniform(1));

    let content_area = block.inner(popup);
    f.render_widget(block, popup);

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // 0. Nombre
            Constraint::Length(3), // 1. Loader
            Constraint::Length(3), // 2. Versión
            Constraint::Length(3), // 3. RAM
            Constraint::Length(3), // 4. Puerto
            Constraint::Length(2), // 5. Error o Progreso
            Constraint::Length(1), // 6. Barra de ayuda
        ])
        .split(content_area);

    // 0. Nombre
    draw_input_field(
        f,
        inner[0],
        " Nombre identificador ",
        app.create_name_input.value(),
        Some("mi-servidor"),
        app.create_field_idx == 0,
        theme,
        app.create_name_input.visual_cursor(),
    );

    // 1. Loader
    draw_loader_selector(
        f,
        inner[1],
        app.create_loader_idx,
        app.create_field_idx == 1,
        theme,
    );

    // 2. Versión o Modpack
    if app.create_loader_idx == 3 {
        let label_owned: String;
        let label: &str = if app.create_modpack_resolving {
            " Modpack / Serverpack (Consultando metadatos...) "
        } else if let Some(ref st) = app.create_modpack_status {
            label_owned = format!(" Modpack / Serverpack [{}] ", st);
            &label_owned
        } else {
            " Modpack / Server Pack (CurseForge, Modrinth o .zip) "
        };

        draw_input_field(
            f,
            inner[2],
            label,
            app.create_modpack_input.value(),
            Some("URL CurseForge, slug/ID Modrinth o enlace directo .zip"),
            app.create_field_idx == 2,
            theme,
            app.create_modpack_input.visual_cursor(),
        );
    } else {
        draw_version_selector(
            f,
            inner[2],
            &app.create_versions,
            app.create_version_idx,
            app.create_loading_versions,
            app.create_field_idx == 2,
            theme,
        );
    }

    // 3. RAM
    draw_input_field(
        f,
        inner[3],
        " Memoria RAM ",
        app.create_ram_input.value(),
        Some("4G"),
        app.create_field_idx == 3,
        theme,
        app.create_ram_input.visual_cursor(),
    );

    // 4. Puerto
    draw_input_field(
        f,
        inner[4],
        " Puerto ",
        app.create_port_input.value(),
        Some("25565"),
        app.create_field_idx == 4,
        theme,
        app.create_port_input.visual_cursor(),
    );

    // 5. Progreso o Error
    if let Some((ratio, ref msg)) = app.create_progress {
        let gauge = Gauge::default()
            .gauge_style(theme.highlight)
            .style(Style::default().bg(Color::Rgb(18, 18, 24)))
            .ratio((ratio / 100.0).clamp(0.0, 1.0))
            .label(msg.as_str());
        f.render_widget(gauge, inner[5]);
    } else if let Some(ref err) = app.create_error {
        let p = Paragraph::new(format!(" ✗ {}", err))
            .style(theme.error)
            .style(Style::default().bg(Color::Rgb(18, 18, 24)).fg(Color::Red));
        f.render_widget(p, inner[5]);
    }

    // 6. Pie de ayuda según InputMode
    let hints = if app.input_mode == InputMode::Insert {
        Line::from(vec![
            Span::styled(" Esc: ", theme.accent),
            Span::styled("Cancelar ", theme.muted),
            Span::styled("│ ", theme.border),
            Span::styled("Tab: ", theme.accent),
            Span::styled("Siguiente ", theme.base),
            Span::styled("│ ", theme.border),
            Span::styled("Enter: ", theme.accent),
            Span::styled("Confirmar", theme.base),
        ])
    } else {
        Line::from(vec![
            Span::styled(" Esc: ", theme.accent),
            Span::styled("Cerrar ", theme.muted),
            Span::styled("│ ", theme.border),
            Span::styled("Tab/↓: ", theme.accent),
            Span::styled("Siguiente ", theme.base),
            Span::styled("│ ", theme.border),
            Span::styled("◄►: ", theme.accent),
            Span::styled("Cambiar ", theme.base),
            Span::styled("│ ", theme.border),
            Span::styled("i: ", theme.accent),
            Span::styled("Editar ", theme.base),
            Span::styled("│ ", theme.border),
            Span::styled("Enter: ", theme.accent),
            Span::styled("Crear", theme.highlight),
        ])
    };
    f.render_widget(Paragraph::new(hints), inner[6]);
}

#[allow(clippy::too_many_arguments)]
fn draw_input_field(
    f: &mut Frame,
    area: Rect,
    label: &str,
    val: &str,
    placeholder: Option<&str>,
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
        .style(Style::default().bg(Color::Rgb(18, 18, 24)))
        .title(label);

    let p = if val.is_empty() {
        if let Some(ph) = placeholder {
            Paragraph::new(Span::styled(format!(" {}", ph), theme.muted)).block(block)
        } else {
            Paragraph::new(" ").block(block)
        }
    } else {
        Paragraph::new(Span::styled(format!(" {}", val), theme.base)).block(block)
    };
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
        .style(Style::default().bg(Color::Rgb(18, 18, 24)))
        .title(" Tipo de Servidor ");

    let loaders = ["Paper", "Fabric", "Vanilla", "Modpack"];
    let mut spans = vec![Span::raw(" ")];
    for (i, name) in loaders.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("   "));
        }
        if i == selected_idx {
            spans.push(Span::styled(format!("[● {}]", name), theme.accent));
        } else {
            spans.push(Span::styled(format!("○ {}", name), theme.muted));
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
        .style(Style::default().bg(Color::Rgb(18, 18, 24)))
        .title(" Versión de Minecraft ");

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
    let target_height = ((r.height as u32 * percent_y as u32) / 100) as u16;
    let actual_height = target_height.max(22).min(r.height.saturating_sub(2));
    let v_margin = (r.height.saturating_sub(actual_height)) / 2;

    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(v_margin),
            Constraint::Length(actual_height),
            Constraint::Min(0),
        ])
        .split(r);

    let target_width = ((r.width as u32 * percent_x as u32) / 100) as u16;
    let actual_width = target_width.max(64).min(r.width.saturating_sub(2));
    let h_margin = (r.width.saturating_sub(actual_width)) / 2;

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(h_margin),
            Constraint::Length(actual_width),
            Constraint::Min(0),
        ])
        .split(popup_layout[1])[1]
}
