//! The settings popup (`,`): three tabs.
//!
//! - Theme: moving through the list previews each theme live; Enter keeps
//!   it, Esc goes back to the saved one.
//! - Keys: Enter, then press the new key for that action. Backspace resets
//!   the action to its default keys.
//! - General: start-up splash on or off.
//!
//! Every change is saved to config.toml straight away through
//! `Effect::Save`, so there is no separate save step to forget.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Widget};

use super::app::{App, Effect, NoticeKind};
use super::fit;
use super::keys::{Action, Key};
use super::overlay::{Overlay, centered};
use super::theme::{PALETTES, Theme};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Theme,
    Keys,
    General,
}

const TABS: [Tab; 3] = [Tab::Theme, Tab::Keys, Tab::General];

pub struct Settings {
    pub tab: Tab,
    pub row: usize,
    /// Waiting for the next key press to bind it to the selected action.
    pub capturing: bool,
    /// The theme in config.toml, restored if the user leaves a preview.
    saved_theme: &'static str,
}

impl Settings {
    pub fn new(current_theme: &'static str) -> Self {
        Self {
            tab: Tab::Theme,
            row: PALETTES
                .iter()
                .position(|p| p.name == current_theme)
                .unwrap_or(0),
            capturing: false,
            saved_theme: current_theme,
        }
    }

    fn rows(&self) -> usize {
        match self.tab {
            Tab::Theme => PALETTES.len(),
            Tab::Keys => Action::ALL.len(),
            Tab::General => 1,
        }
    }
}

impl App {
    pub(super) fn on_settings_key(
        &mut self,
        mut settings: Settings,
        key: KeyEvent,
    ) -> (Option<Overlay>, Option<Effect>) {
        if settings.capturing {
            settings.capturing = false;
            if key.code == KeyCode::Esc {
                return (Some(Overlay::Settings(settings)), None);
            }
            let pressed = Key::from_event(key);
            if pressed.is_reserved() {
                self.notify(NoticeKind::Error, format!("{pressed} is reserved."));
                return (Some(Overlay::Settings(settings)), None);
            }
            let action = Action::ALL[settings.row];
            self.keymap.rebind(action, pressed);
            return (Some(Overlay::Settings(settings)), self.save());
        }

        let mut effect = None;
        match key.code {
            KeyCode::Esc => {
                self.theme = Theme::named(settings.saved_theme);
                return (None, None);
            }
            KeyCode::Tab | KeyCode::Right | KeyCode::BackTab | KeyCode::Left => {
                let step = if matches!(key.code, KeyCode::Tab | KeyCode::Right) {
                    1
                } else {
                    2
                };
                let at = TABS.iter().position(|t| *t == settings.tab).unwrap_or(0);
                settings.tab = TABS[(at + step) % TABS.len()];
                settings.row = 0;
                self.theme = Theme::named(settings.saved_theme);
            }
            KeyCode::Up | KeyCode::Char('k') => {
                settings.row = settings.row.saturating_sub(1);
                self.preview(&settings);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                settings.row = (settings.row + 1).min(settings.rows() - 1);
                self.preview(&settings);
            }
            KeyCode::Enter | KeyCode::Char(' ') => match settings.tab {
                Tab::Theme => {
                    settings.saved_theme = PALETTES[settings.row].name;
                    effect = self.save();
                }
                Tab::Keys => settings.capturing = true,
                Tab::General => {
                    self.config.ui.splash = !self.config.ui.splash;
                    effect = self.save();
                }
            },
            KeyCode::Backspace | KeyCode::Delete if settings.tab == Tab::Keys => {
                self.keymap.reset(Action::ALL[settings.row]);
                effect = self.save();
            }
            _ => {}
        }
        (Some(Overlay::Settings(settings)), effect)
    }

    fn preview(&mut self, settings: &Settings) {
        if settings.tab == Tab::Theme {
            self.theme = Theme::named(PALETTES[settings.row].name);
        }
    }

    fn save(&mut self) -> Option<Effect> {
        let config = self.config_to_save();
        self.config = config.clone();
        Some(Effect::Save(config))
    }
}

pub fn render(app: &App, settings: &Settings, area: Rect, buf: &mut Buffer) {
    let theme = app.theme;
    let mut lines = Vec::new();

    let mut tabs = vec![Span::raw(" ")];
    for tab in TABS {
        let label = match tab {
            Tab::Theme => " Theme ",
            Tab::Keys => " Keys ",
            Tab::General => " General ",
        };
        let style = if tab == settings.tab {
            theme.selected()
        } else {
            theme.dim()
        };
        tabs.push(Span::styled(label, style));
        tabs.push(Span::raw(" "));
    }
    lines.push(Line::from(tabs));
    lines.push(Line::raw(""));

    let row_style = |i: usize| {
        if i == settings.row {
            theme.selected()
        } else {
            theme.text()
        }
    };
    match settings.tab {
        Tab::Theme => {
            for (i, palette) in PALETTES.iter().enumerate() {
                let saved = if palette.name == settings.saved_theme {
                    "  saved"
                } else {
                    ""
                };
                lines.push(Line::from(vec![
                    Span::styled(format!("  {}", fit(palette.name, 20)), row_style(i)),
                    Span::styled(saved, theme.success()),
                ]));
            }
            lines.push(Line::raw(""));
            lines.push(Line::styled(
                "  ↑↓ preview   Enter keep   Esc back to saved",
                theme.dim(),
            ));
        }
        Tab::Keys => {
            let visible = 14usize;
            let first = settings.row.saturating_sub(visible - 1);
            for (i, action) in Action::ALL.iter().enumerate().skip(first).take(visible) {
                let keys = if settings.capturing && i == settings.row {
                    Span::styled("press a key...", theme.warning())
                } else {
                    Span::styled(fit(&app.keymap.describe(*action), 12), theme.key())
                };
                lines.push(Line::from(vec![
                    Span::styled(format!("  {}", fit(action.description(), 40)), row_style(i)),
                    Span::raw(" "),
                    keys,
                ]));
            }
            lines.push(Line::raw(""));
            lines.push(Line::styled(
                "  Enter rebind   Backspace default   Esc close",
                theme.dim(),
            ));
        }
        Tab::General => {
            let state = if app.config.ui.splash { "on" } else { "off" };
            lines.push(Line::from(vec![
                Span::styled(format!("  {}", fit("Splash at start-up", 40)), row_style(0)),
                Span::styled(format!(" {state}"), theme.key()),
            ]));
            lines.push(Line::raw(""));
            lines.push(Line::styled("  Enter toggle   Esc close", theme.dim()));
        }
    }
    let path = crate::config::path()
        .map(|p| super::tilde(&p))
        .unwrap_or_else(|_| "no config folder found".into());
    lines.push(Line::styled(format!("  Saved to {path}"), theme.dim()));

    let popup = centered(area, 66, lines.len() as u16 + 3);
    Clear.render(popup, buf);
    Paragraph::new(lines)
        .block(
            Block::bordered()
                .title(Span::styled(" Settings ", theme.title()))
                .border_style(theme.focused_border())
                .style(theme.base()),
        )
        .render(popup, buf);
}
