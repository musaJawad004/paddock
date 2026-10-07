//! Popups that take over the keyboard: help, settings, a yes/no
//! confirmation, a one-line text input, and a list picker.
//!
//! Each popup handles its own keys here; the settings popup lives in
//! `settings.rs` because it is bigger. Esc always closes without doing
//! anything. Popups never change backend state themselves: they return an
//! `Effect` once the user confirms.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Widget, Wrap};

use super::app::{App, Effect};
use super::folders::FolderPicker;
use super::settings::Settings;
use crate::ipc::protocol::Request;
use crate::model::ProcessId;

pub const NEW_PROJECT: &str = "+ New project...";

pub enum Overlay {
    Help,
    Settings(Settings),
    Confirm { message: String, then: Confirmed },
    Input(Input),
    Picker(Picker),
    Folders(FolderPicker),
}

pub struct Input {
    pub title: String,
    pub value: String,
    pub hint: String,
    pub error: Option<String>,
    pub purpose: InputPurpose,
}

pub enum InputPurpose {
    Port(ProcessId),
    NewProject(ProcessId),
}

/// What a yes in a confirmation does.
pub enum Confirmed {
    Send(Request),
    Quit,
}

pub struct Picker {
    pub title: String,
    pub items: Vec<String>,
    pub selected: usize,
    pub target: ProcessId,
}

impl App {
    pub(super) fn on_overlay_key(&mut self, key: KeyEvent) -> Option<Effect> {
        let overlay = self.overlay.take()?;
        let (keep, effect) = match overlay {
            Overlay::Help => (None, None),
            Overlay::Settings(settings) => self.on_settings_key(settings, key),
            Overlay::Confirm { message, then } => match key.code {
                KeyCode::Char('y' | 'Y') | KeyCode::Enter => match then {
                    Confirmed::Send(request) => (None, Some(Effect::Send(request))),
                    Confirmed::Quit => {
                        self.quit_now();
                        (None, None)
                    }
                },
                KeyCode::Char('n' | 'N') | KeyCode::Esc => (None, None),
                _ => (Some(Overlay::Confirm { message, then }), None),
            },
            Overlay::Input(input) => on_input_key(input, key),
            Overlay::Picker(picker) => self.on_picker_key(picker, key),
            Overlay::Folders(picker) => self.on_folder_key(picker, key),
        };
        self.overlay = keep;
        effect
    }

    fn on_picker_key(
        &mut self,
        mut picker: Picker,
        key: KeyEvent,
    ) -> (Option<Overlay>, Option<Effect>) {
        let last = picker.items.len().saturating_sub(1);
        match key.code {
            KeyCode::Esc => return (None, None),
            KeyCode::Up | KeyCode::Char('k') => picker.selected = picker.selected.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => picker.selected = (picker.selected + 1).min(last),
            KeyCode::Enter => {
                let choice = picker
                    .items
                    .get(picker.selected)
                    .cloned()
                    .unwrap_or_default();
                if choice == NEW_PROJECT {
                    let input = Input {
                        title: format!("New project for {}", picker.target),
                        value: String::new(),
                        hint: "A name for the new project.".into(),
                        error: None,
                        purpose: InputPurpose::NewProject(picker.target),
                    };
                    return (Some(Overlay::Input(input)), None);
                }
                let request = Request::Move {
                    id: picker.target,
                    project: choice,
                };
                return (None, Some(Effect::Send(request)));
            }
            _ => {}
        }
        (Some(Overlay::Picker(picker)), None)
    }
}

fn on_input_key(mut input: Input, key: KeyEvent) -> (Option<Overlay>, Option<Effect>) {
    match key.code {
        KeyCode::Esc => return (None, None),
        KeyCode::Backspace => {
            input.value.pop();
        }
        KeyCode::Char(c) if input.value.chars().count() < 40 => {
            let allowed = match input.purpose {
                InputPurpose::Port(_) => c.is_ascii_digit(),
                InputPurpose::NewProject(_) => !c.is_control() && c != '/',
            };
            if allowed {
                input.value.push(c);
            }
        }
        KeyCode::Enter => match submit(&input) {
            Ok(request) => return (None, Some(Effect::Send(request))),
            Err(error) => input.error = Some(error),
        },
        _ => {}
    }
    (Some(Overlay::Input(input)), None)
}

fn submit(input: &Input) -> Result<Request, String> {
    match &input.purpose {
        InputPurpose::Port(id) => match input.value.parse::<u16>() {
            Ok(port) if port > 0 => Ok(Request::SetPort {
                id: id.clone(),
                port,
            }),
            _ => Err("Enter a number from 1 to 65535.".into()),
        },
        InputPurpose::NewProject(id) => {
            let name = input.value.trim();
            if name.is_empty() {
                return Err("The name cannot be empty.".into());
            }
            Ok(Request::Move {
                id: id.clone(),
                project: name.to_owned(),
            })
        }
    }
}

/// Draws confirm, input and picker popups.
pub fn render(app: &App, overlay: &Overlay, area: Rect, buf: &mut Buffer) {
    let theme = app.theme;
    let (title, lines, width) = match overlay {
        Overlay::Confirm { message, .. } => {
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
            ("Are you sure?".to_owned(), lines, 60)
        }
        Overlay::Input(input) => {
            let mut lines = vec![
                Line::raw(""),
                Line::from(vec![
                    Span::styled(" > ", theme.accent()),
                    Span::styled(tail(&input.value, 48), theme.text()),
                    Span::styled("█", theme.accent()),
                ]),
                Line::raw(""),
                Line::styled(format!(" {}", input.hint), theme.dim()),
            ];
            if let Some(error) = &input.error {
                lines.push(Line::styled(format!(" {error}"), theme.error()));
            }
            lines.push(Line::styled(" Enter to save, Esc to cancel", theme.dim()));
            (input.title.clone(), lines, 56)
        }
        Overlay::Picker(picker) => {
            let mut lines = vec![Line::raw("")];
            for (i, item) in picker.items.iter().enumerate() {
                let style = if i == picker.selected {
                    theme.selected()
                } else {
                    theme.text()
                };
                lines.push(Line::styled(format!("  {item}  "), style));
            }
            lines.push(Line::raw(""));
            lines.push(Line::styled(" Enter to move, Esc to cancel", theme.dim()));
            (picker.title.clone(), lines, 48)
        }
        Overlay::Help | Overlay::Settings(_) | Overlay::Folders(_) => return,
    };

    let height = lines.len() as u16 + 3;
    let popup = centered(area, width, height);
    Clear.render(popup, buf);
    Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .block(
            Block::bordered()
                .title(Span::styled(format!(" {title} "), theme.title()))
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

/// The end of `text`, so the cursor end of a long path stays visible.
fn tail(text: &str, max: usize) -> String {
    let count = text.chars().count();
    if count <= max {
        return text.to_owned();
    }
    let rest: String = text.chars().skip(count - max + 1).collect();
    format!("…{rest}")
}
