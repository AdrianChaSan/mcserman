use ratatui::style::{Color, Modifier, Style};

#[allow(dead_code)]
pub struct Theme {
    pub base: Style,
    pub muted: Style,
    pub accent: Style,
    pub bold: Style,
    pub error: Style,
    pub success: Style,
    pub warning: Style,
    pub border: Style,
    pub active_border: Style,
    pub highlight: Style,
    pub header: Style,
}

impl Default for Theme {
    fn default() -> Self {
        Self::new()
    }
}

impl Theme {
    pub fn new() -> Self {
        Self {
            base: Style::default().fg(Color::White),
            muted: Style::default().fg(Color::DarkGray),
            accent: Style::default().fg(Color::Cyan),
            bold: Style::default().add_modifier(Modifier::BOLD),
            error: Style::default().fg(Color::Red),
            success: Style::default().fg(Color::Green),
            warning: Style::default().fg(Color::Yellow),
            border: Style::default().fg(Color::DarkGray),
            active_border: Style::default().fg(Color::Cyan),
            highlight: Style::default()
                .bg(Color::Rgb(32, 40, 52))
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
            header: Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        }
    }
}

pub const HEADER_HEIGHT: u16 = 3;
pub const STATUS_BAR_HEIGHT: u16 = 1;
pub const SIDEBAR_WIDTH: u16 = 26;
pub const MIN_WIDTH: u16 = 60;
pub const MIN_HEIGHT: u16 = 16;
