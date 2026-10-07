//! Every colour and style the TUI uses. View code asks the theme, never
//! builds a `Color` itself.
//!
//! Status colours are fixed: running green, starting yellow, crashed red,
//! stopped dim. One accent colour for focus and selection. With `NO_COLOR`
//! set, only bold, dim and reverse are used.

use ratatui::style::{Color, Modifier, Style};

use crate::model::ProcessState;

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    color: bool,
}

impl Theme {
    pub fn from_env() -> Self {
        Self {
            color: std::env::var_os("NO_COLOR").is_none_or(|v| v.is_empty()),
        }
    }

    #[cfg(test)]
    pub fn plain() -> Self {
        Self { color: false }
    }

    fn fg(self, color: Color) -> Style {
        if self.color {
            Style::new().fg(color)
        } else {
            Style::new()
        }
    }

    pub fn accent(self) -> Style {
        self.fg(Color::Cyan).add_modifier(Modifier::BOLD)
    }

    pub fn border(self) -> Style {
        self.fg(Color::DarkGray)
    }

    pub fn focused_border(self) -> Style {
        self.fg(Color::Cyan)
    }

    pub fn selected(self) -> Style {
        if self.color {
            Style::new()
                .bg(Color::Rgb(38, 50, 66))
                .add_modifier(Modifier::BOLD)
        } else {
            Style::new().add_modifier(Modifier::REVERSED)
        }
    }

    pub fn dim(self) -> Style {
        self.fg(Color::DarkGray)
    }

    pub fn title(self) -> Style {
        Style::new().add_modifier(Modifier::BOLD)
    }

    pub fn key(self) -> Style {
        self.fg(Color::Cyan).add_modifier(Modifier::BOLD)
    }

    pub fn error(self) -> Style {
        self.fg(Color::Red)
    }

    pub fn warning(self) -> Style {
        self.fg(Color::Yellow)
    }

    pub fn state(self, state: ProcessState) -> Style {
        match state {
            ProcessState::Running => self.fg(Color::Green),
            ProcessState::Starting | ProcessState::Stopping => self.fg(Color::Yellow),
            ProcessState::Crashed(_) => self.fg(Color::Red).add_modifier(Modifier::BOLD),
            ProcessState::Stopped | ProcessState::Exited => self.dim(),
        }
    }

    pub fn glyph(state: ProcessState) -> &'static str {
        match state {
            ProcessState::Running => "●",
            ProcessState::Starting | ProcessState::Stopping => "◐",
            ProcessState::Crashed(_) => "✗",
            ProcessState::Stopped | ProcessState::Exited => "○",
        }
    }
}
