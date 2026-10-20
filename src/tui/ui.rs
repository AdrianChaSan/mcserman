use crate::tui::app::{App, AppTab, InputMode};
use crate::tui::theme::{
    Theme, HEADER_HEIGHT, MIN_HEIGHT, MIN_WIDTH, SIDEBAR_WIDTH, STATUS_BAR_HEIGHT,
};
use crate::tui::views;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Tabs};

pub fn draw(f: &mut Frame, app: &App) {
    let theme = Theme::new();
    let area = f.area();

    // 1. Verificación de tamaño mínimo de terminal
    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        let msg = format!(
            "Terminal demasiado pequeña ({}x{}). Mínimo requerido: {}x{}",
            area.width, area.height, MIN_WIDTH, MIN_HEIGHT
        );
        let p = Paragraph::new(msg)
            .style(theme.warning)
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(theme.error),
            );
        f.render_widget(p, area);
        return;
    }

    // 2. División Vertical: Header + Contenido + Footer
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(HEADER_HEIGHT),
            Constraint::Min(0),
            Constraint::Length(STATUS_BAR_HEIGHT),
        ])
        .split(area);

    draw_header(f, chunks[0], app, &theme);
    draw_body(f, chunks[1], app, &theme);
    draw_footer(f, chunks[2], app, &theme);

    // 3. Modal de Creación (si está abierto)
    if app.show_create_modal {
        views::create::draw_modal(f, area, app, &theme);
    }
}

fn draw_header(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let mode_str = if app.input_mode == InputMode::Insert {
        Span::styled("[MODO INSERT] ", theme.accent)
    } else {
        Span::styled("[MODO NORMAL] ", theme.muted)
    };

    let title_line = Line::from(vec![
        Span::styled(" ⛏️  ", theme.bold),
        Span::styled("mcserman", theme.header),
        Span::styled(" — Gestor de Servidores Minecraft ", theme.base),
        Span::styled("(NixOS/Linux) ", theme.muted),
        mode_str,
    ]);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.border);

    let p = Paragraph::new(title_line).block(block);
    f.render_widget(p, area);
}

fn draw_body(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(SIDEBAR_WIDTH), Constraint::Min(0)])
        .split(area);

    draw_sidebar(f, columns[0], app, theme);
    draw_main_panel(f, columns[1], app, theme);
}

fn draw_sidebar(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.border)
        .title(format!(" 📦 Servidores ({}) ", app.instances.len()));

    if app.instances.is_empty() {
        let empty_msg = Paragraph::new(" Sin instancias\n Presiona [n] para crear")
            .style(theme.muted)
            .block(block);
        f.render_widget(empty_msg, area);
        return;
    }

    let items: Vec<ListItem> = app
        .instances
        .iter()
        .enumerate()
        .map(|(i, inst)| {
            let is_selected = i == app.selected_instance;
            let pointer = if is_selected { "▸ " } else { "  " };

            let status_dot = if inst.is_online {
                Span::styled("● ", theme.success)
            } else {
                Span::styled("○ ", theme.muted)
            };

            let name_style = if is_selected {
                theme.highlight
            } else {
                theme.base
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
                status_dot,
                Span::styled(&inst.name, name_style),
            ]);

            ListItem::new(line)
        })
        .collect();

    let list = List::new(items).block(block);
    f.render_widget(list, area);
}

fn draw_main_panel(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    // Barra de Tabs
    let tab_titles: Vec<Line> = AppTab::ALL
        .iter()
        .map(|t| {
            let style = if *t == app.active_tab {
                theme.accent
            } else {
                theme.muted
            };
            Line::from(Span::styled(t.title(), style))
        })
        .collect();

    let tabs = Tabs::new(tab_titles)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(theme.border),
        )
        .select(app.active_tab as usize)
        .highlight_style(theme.accent);

    f.render_widget(tabs, chunks[0]);

    // Contenido de la pestaña activa delegada a submódulos de views/
    match app.active_tab {
        AppTab::Dashboard => views::dashboard::draw(f, chunks[1], app, theme),
        AppTab::Console => views::console::draw(f, chunks[1], app, theme),
        AppTab::Modrinth => views::modrinth::draw(f, chunks[1], app, theme),
        AppTab::Settings => views::properties::draw(f, chunks[1], app, theme),
    }
}

fn draw_footer(f: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let status_text = if let Some((ref msg, _)) = app.status_message {
        Span::styled(format!(" ℹ️ {} ", msg), theme.accent)
    } else {
        Span::styled(" mcserman v0.1.0 ", theme.muted)
    };

    let hints = if app.input_mode == InputMode::Insert {
        Span::styled(
            " Esc: Salir a Modo Normal │ Enter: Confirmar/Enviar",
            theme.accent,
        )
    } else {
        match app.active_tab {
            AppTab::Dashboard => Span::styled(" q: Salir │ j/k: Servidor │ Space: Iniciar/Parar │ n: Nuevo │ 1-4: Tabs", theme.base),
            AppTab::Console => Span::styled(" q: Salir │ i/Enter: Escribir comando │ PgUp/PgDn: Desplazar historial │ 1-4: Tabs", theme.base),
            AppTab::Modrinth => Span::styled(" q: Salir │ i: Buscar │ j/k: Resultados │ Enter: Instalar │ 1-4: Tabs", theme.base),
            AppTab::Settings => Span::styled(" q: Salir │ j/k: Config │ Space/Enter: Cambiar valor │ s: Guardar │ 1-4: Tabs", theme.base),
        }
    };

    let line = Line::from(vec![hints, Span::raw("   "), status_text]);

    let p = Paragraph::new(line).style(Style::default().bg(Color::Rgb(24, 28, 36)));
    f.render_widget(p, area);
}
