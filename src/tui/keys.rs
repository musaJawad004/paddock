//! The key table: one list of keys, action and help text. The input handler,
//! the status bar and the help popup all read it, so help can never drift
//! from behaviour.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Up,
    Down,
    Start,
    Stop,
    Restart,
    ScrollUp,
    ScrollDown,
    Follow,
    Help,
    Quit,
}

pub struct Binding {
    pub codes: &'static [KeyCode],
    pub action: Action,
    /// Shown in the help popup, e.g. "↑ k".
    pub keys: &'static str,
    pub description: &'static str,
    /// Key and label for the status bar. Keep these few.
    pub hint: Option<(&'static str, &'static str)>,
}

pub const BINDINGS: &[Binding] = &[
    Binding {
        codes: &[KeyCode::Up, KeyCode::Char('k')],
        action: Action::Up,
        keys: "↑ k",
        description: "select previous process",
        hint: None,
    },
    Binding {
        codes: &[KeyCode::Down, KeyCode::Char('j')],
        action: Action::Down,
        keys: "↓ j",
        description: "select next process",
        hint: Some(("↑↓", "move")),
    },
    Binding {
        codes: &[KeyCode::Char('s')],
        action: Action::Start,
        keys: "s",
        description: "start the selected process",
        hint: Some(("s", "start")),
    },
    Binding {
        codes: &[KeyCode::Char('x')],
        action: Action::Stop,
        keys: "x",
        description: "stop the selected process",
        hint: Some(("x", "stop")),
    },
    Binding {
        codes: &[KeyCode::Char('r')],
        action: Action::Restart,
        keys: "r",
        description: "restart the selected process",
        hint: Some(("r", "restart")),
    },
    Binding {
        codes: &[KeyCode::PageUp, KeyCode::Char('u')],
        action: Action::ScrollUp,
        keys: "PgUp u",
        description: "scroll logs up",
        hint: Some(("PgUp/PgDn", "scroll")),
    },
    Binding {
        codes: &[KeyCode::PageDown, KeyCode::Char('d')],
        action: Action::ScrollDown,
        keys: "PgDn d",
        description: "scroll logs down",
        hint: None,
    },
    Binding {
        codes: &[KeyCode::End, KeyCode::Char('G')],
        action: Action::Follow,
        keys: "End G",
        description: "jump to the newest log line",
        hint: None,
    },
    Binding {
        codes: &[KeyCode::Char('?')],
        action: Action::Help,
        keys: "?",
        description: "show or hide this help",
        hint: Some(("?", "help")),
    },
    Binding {
        codes: &[KeyCode::Char('q'), KeyCode::Esc],
        action: Action::Quit,
        keys: "q Esc",
        description: "quit (Esc closes help first)",
        hint: Some(("q", "quit")),
    },
];

pub fn action_for(key: KeyEvent) -> Option<Action> {
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        return (key.code == KeyCode::Char('c')).then_some(Action::Quit);
    }
    BINDINGS
        .iter()
        .find(|b| b.codes.contains(&key.code))
        .map(|b| b.action)
}
