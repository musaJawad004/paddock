//! Popups that take over the keyboard: help, settings, and a yes/no
//! confirmation before anything is stopped.
//!
//! Each popup handles its own keys here; the settings popup lives in
//! `settings.rs` because it is bigger. Esc always closes without doing
//! anything. A confirmation returns its `Request` only on yes.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Widget, Wrap};

use super::app::{App, Effect};
use super::settings::Settings;
use crate::ipc::protocol::Request;

pub enum Overlay {
    Help,
    Settings(Settings),
    Confirm { message: String, request: Request },
}

impl App {
    pub(super) fn on_overlay_key(&mut self, key: KeyEvent) -> Option<Effect> {
        let overlay = self.overlay.take()?;
        let (keep, effect) = match overlay {
            Overlay::Help => (None, None),
            Overlay::Settings(settings) => self.on_settings_key(settings, key),
            Overlay::Confirm { message, request } => match key.code {
                KeyCode::Char('y' | 'Y') | KeyCode::Enter => (None, Some(Effect::Send(request))),
                KeyCode::Char('n' | 'N') | KeyCode::Esc => (None, None),
                _ => (Some(Overlay::Confirm { message, request }), None),
            },
        };
        self.overlay = keep;
        effect
    }
}

/// Draws the confirmation popup.
pub fn render(app: &App, overlay: &Overlay, area: Rect, buf: &mut Buffer) {
    let Overlay::Confirm { message, .. } = overlay else {
        return;
    };
    let theme = app.theme;
    let lines = vec![
        Line::raw(""),
        Line::styled(format!(" {message}"), theme.text()),
        Line::raw(""),
        Line::from(vec![
            Span::styled(" y", theme.key()),
            Span::styled(" yes    ", theme.dim()),
            Span::styled("n", theme.key()),
            Span::styled(" no", theme.dim()),
        ]),
    ];
    let popup = centered(area, 62, 9);
    Clear.render(popup, buf);
    Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .block(
            Block::bordered()
                .title(Span::styled(" Are you sure? ", theme.title()))
                .border_style(theme.focused_border())
                .style(theme.base()),
        )
        .render(popup, buf);
}

pub fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let [popup] = Layout::horizontal([Constraint::Length(width.min(area.width))])
        .flex(Flex::Center)
        .areas(area);
    let [popup] = Layout::vertical([Constraint::Length(height.min(area.height))])
        .flex(Flex::Center)
        .areas(popup);
    popup
}
