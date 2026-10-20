use crate::tui::app::App;
use crate::tui::theme::Theme;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};

pub fn draw(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.border)
        .title(" Resumen de Servidor ");

    if let Some(inst) = app.selected() {
        let status_span = if inst.is_online {
            Span::styled("● ONLINE (En ejecución)", theme.success)
        } else {
            Span::styled("○ OFFLINE (Detenido)", theme.muted)
        };

        let lines = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("  Instancia:    ", theme.muted),
                Span::styled(&inst.name, theme.bold),
            ]),
            Line::from(vec![
                Span::styled("  Estado:       ", theme.muted),
                status_span,
            ]),
            Line::from(vec![
                Span::styled("  Modloader:    ", theme.muted),
                Span::styled(&inst.loader, theme.accent),
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
            Line::from(""),
            Line::from(vec![
                Span::styled("  Acciones:     ", theme.muted),
                Span::styled("[Space] ", theme.accent),
                Span::styled("Iniciar/Detener   ", theme.base),
                Span::styled("[2] ", theme.accent),
                Span::styled("Abrir Consola   ", theme.base),
                Span::styled("[3] ", theme.accent),
                Span::styled("Mods/Plugins   ", theme.base),
                Span::styled("[n] ", theme.accent),
                Span::styled("Nuevo Servidor", theme.base),
            ]),
        ];

        let p = Paragraph::new(lines).block(block);
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
        let p = Paragraph::new(empty_lines).block(block);
        f.render_widget(p, area);
    }
}
