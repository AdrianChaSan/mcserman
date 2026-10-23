use crate::tui::app::{App, Focus, ServerState};
use crate::tui::theme::Theme;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

/// Renderiza la vista de resumen con el estado, configuración del servidor y detalles del modpack.
pub fn draw(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let border_style = if app.focus == Focus::MainPanel {
        theme.active_border
    } else {
        theme.border
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(" Resumen de Servidor ");

    if let Some(inst) = app.selected() {
        let status_badge = match inst.state {
            ServerState::Online => Span::styled(
                " [ ● ONLINE ] ",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            ServerState::Starting => Span::styled(
                " [ ◐ CARGANDO... ] ",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            ServerState::Stopping => Span::styled(
                " [ ◐ DETENIENDO... ] ",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            ServerState::Offline => Span::styled(
                " [ ○ OFFLINE ] ",
                Style::default()
                    .fg(Color::LightRed)
                    .add_modifier(Modifier::BOLD),
            ),
        };

        let loader_display = if let Some(ref lv) = inst.loader_version {
            format!("{} {}", inst.loader, lv)
        } else {
            inst.loader.clone()
        };

        let mut lines = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("  Instancia:    ", theme.muted),
                Span::styled(&inst.name, theme.bold),
            ]),
            Line::from(vec![
                Span::styled("  Estado:       ", theme.muted),
                status_badge,
            ]),
            Line::from(vec![
                Span::styled("  Modloader:    ", theme.muted),
                Span::styled(loader_display, theme.accent),
                Span::styled(" (Minecraft ", theme.muted),
                Span::styled(&inst.mc_version, theme.base),
                Span::styled(")", theme.muted),
            ]),
            Line::from(vec![
                Span::styled("  Memoria RAM:  ", theme.muted),
                Span::styled(&inst.ram, theme.base),
            ]),
            Line::from(vec![
                Span::styled("  Puerto TCP:   ", theme.muted),
                Span::styled(inst.port.to_string(), theme.base),
            ]),
            Line::from(vec![
                Span::styled("  Ruta datos:   ", theme.muted),
                Span::styled(
                    app.paths.instance_dir(&inst.name).display().to_string(),
                    theme.muted,
                ),
            ]),
        ];

        if let Some(ref mp) = inst.modpack_info {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("  ── Información del Modpack ──────────────────────────────", theme.muted),
            ]));
            lines.push(Line::from(vec![
                Span::styled("  Nombre modpack: ", theme.muted),
                Span::styled(&mp.name, theme.bold),
            ]));
            if !mp.version.is_empty() {
                lines.push(Line::from(vec![
                    Span::styled("  Versión actual: ", theme.muted),
                    Span::styled(&mp.version, theme.accent),
                ]));
            }
            let mut provider_parts = vec![
                Span::styled("  Proveedor:      ", theme.muted),
                Span::styled(&mp.provider, theme.base),
            ];
            if let Some(ref pid) = mp.project_id {
                provider_parts.push(Span::styled(" │ Pack ID: ", theme.muted));
                provider_parts.push(Span::styled(pid, theme.base));
            }
            if let Some(ref fid) = mp.file_id {
                provider_parts.push(Span::styled(" │ Version ID: ", theme.muted));
                provider_parts.push(Span::styled(fid, theme.base));
            }
            lines.push(Line::from(provider_parts));
        }

        let p = Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false });
        f.render_widget(p, area);
    } else {
        let empty_lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                "  No hay instancias seleccionadas.",
                theme.muted,
            )),
            Line::from(Span::styled(
                "  Presiona [n] para crear una nueva instancia con el asistente.",
                theme.base,
            )),
        ];
        let p = Paragraph::new(empty_lines)
            .block(block)
            .wrap(Wrap { trim: false });
        f.render_widget(p, area);
    }
}
