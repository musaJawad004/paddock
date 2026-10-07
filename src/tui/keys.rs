//! Actions and the keys bound to them.
//!
//! `Keymap::defaults()` is the built-in layout. The user's `[keys]` table in
//! config.toml replaces the keys of the actions it names, and the settings
//! screen edits the same table. The input handler, the status bar and the
//! help popup all read the keymap, so help can never drift from behaviour.
//!
//! Fixed keys that cannot be rebound: Ctrl+C quits, Esc cancels, Enter
//! confirms. They keep the UI usable whatever the config says.
//!
//! Key names: a single character (`s`, `K`, `?`), or `enter`, `esc`, `tab`,
//! `backtab`, `space`, `backspace`, `delete`, `up`, `down`, `left`, `right`,
//! `home`, `end`, `pgup`, `pgdn`, `f1` to `f12`, with optional `ctrl+` and
//! `alt+` prefixes. `shift+tab` means `backtab`; for letters, use the
//! uppercase letter instead of `shift+`.

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::config::Keys;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Action {
    Up,
    Down,
    NextPane,
    PrevPane,
    Open,
    Copy,
    CopyAll,
    Mark,
    ScrollUp,
    ScrollDown,
    Follow,
    Stop,
    Kill,
    Settings,
    Help,
    Quit,
}

impl Action {
    pub const ALL: [Action; 16] = [
        Action::Up,
        Action::Down,
        Action::NextPane,
        Action::PrevPane,
        Action::Open,
        Action::Copy,
        Action::CopyAll,
        Action::Mark,
        Action::ScrollUp,
        Action::ScrollDown,
        Action::Follow,
        Action::Stop,
        Action::Kill,
        Action::Settings,
        Action::Help,
        Action::Quit,
    ];

    /// The name used in config.toml.
    pub fn name(self) -> &'static str {
        match self {
            Action::Up => "up",
            Action::Down => "down",
            Action::NextPane => "next_pane",
            Action::PrevPane => "prev_pane",
            Action::Open => "open",
            Action::Copy => "copy",
            Action::CopyAll => "copy_all",
            Action::Mark => "mark",
            Action::ScrollUp => "scroll_up",
            Action::ScrollDown => "scroll_down",
            Action::Follow => "follow",
            Action::Stop => "stop",
            Action::Kill => "kill",
            Action::Settings => "settings",
            Action::Help => "help",
            Action::Quit => "quit",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Action::Up => "move up",
            Action::Down => "move down",
            Action::NextPane => "next pane",
            Action::PrevPane => "previous pane",
            Action::Open => "open in the browser",
            Action::Copy => "copy URL / log lines",
            Action::CopyAll => "copy command / all logs",
            Action::Mark => "select log lines",
            Action::ScrollUp => "logs: page up",
            Action::ScrollDown => "logs: page down",
            Action::Follow => "logs: jump to newest",
            Action::Stop => "stop the server",
            Action::Kill => "kill it now",
            Action::Settings => "settings",
            Action::Help => "help",
            Action::Quit => "quit",
        }
    }

    fn default_keys(self) -> &'static [&'static str] {
        match self {
            Action::Up => &["up", "k"],
            Action::Down => &["down", "j"],
            Action::NextPane => &["tab"],
            Action::PrevPane => &["backtab"],
            Action::Open => &["o"],
            Action::Copy => &["y"],
            Action::CopyAll => &["Y"],
            Action::Mark => &["v"],
            Action::ScrollUp => &["pgup", "u"],
            Action::ScrollDown => &["pgdn", "d"],
            Action::Follow => &["end", "G"],
            Action::Stop => &["x"],
            Action::Kill => &["K"],
            Action::Settings => &[","],
            Action::Help => &["?"],
            Action::Quit => &["q"],
        }
    }

    fn from_name(name: &str) -> Option<Action> {
        Action::ALL.into_iter().find(|a| a.name() == name)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Key {
    code: KeyCode,
    ctrl: bool,
    alt: bool,
}

impl Key {
    pub fn from_event(event: KeyEvent) -> Self {
        Self {
            code: event.code,
            ctrl: event.modifiers.contains(KeyModifiers::CONTROL),
            alt: event.modifiers.contains(KeyModifiers::ALT),
        }
    }

    /// Keys the user may not bind, because the UI relies on them.
    pub fn is_reserved(self) -> bool {
        matches!(self.code, KeyCode::Esc | KeyCode::Enter)
            || (self.ctrl && self.code == KeyCode::Char('c'))
    }
}

impl FromStr for Key {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let mut ctrl = false;
        let mut alt = false;
        let mut rest = text;
        loop {
            let lower = rest.to_ascii_lowercase();
            if let Some(stripped) = lower.strip_prefix("ctrl+") {
                ctrl = true;
                rest = &rest[rest.len() - stripped.len()..];
            } else if let Some(stripped) = lower.strip_prefix("alt+") {
                alt = true;
                rest = &rest[rest.len() - stripped.len()..];
            } else {
                break;
            }
        }
        let code = match rest.to_ascii_lowercase().as_str() {
            "enter" | "return" => KeyCode::Enter,
            "esc" | "escape" => KeyCode::Esc,
            "tab" => KeyCode::Tab,
            "backtab" | "shift+tab" => KeyCode::BackTab,
            "space" => KeyCode::Char(' '),
            "backspace" => KeyCode::Backspace,
            "delete" | "del" => KeyCode::Delete,
            "up" => KeyCode::Up,
            "down" => KeyCode::Down,
            "left" => KeyCode::Left,
            "right" => KeyCode::Right,
            "home" => KeyCode::Home,
            "end" => KeyCode::End,
            "pgup" | "pageup" => KeyCode::PageUp,
            "pgdn" | "pagedown" => KeyCode::PageDown,
            lower => {
                if let Some(n) = lower.strip_prefix('f').and_then(|n| n.parse::<u8>().ok())
                    && (1..=12).contains(&n)
                {
                    KeyCode::F(n)
                } else {
                    let mut chars = rest.chars();
                    match (chars.next(), chars.next()) {
                        (Some(c), None) => KeyCode::Char(c),
                        _ => return Err(format!("unknown key \"{text}\"")),
                    }
                }
            }
        };
        Ok(Self { code, ctrl, alt })
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.ctrl {
            f.write_str("ctrl+")?;
        }
        if self.alt {
            f.write_str("alt+")?;
        }
        match self.code {
            KeyCode::Char(' ') => f.write_str("space"),
            KeyCode::Char(c) => write!(f, "{c}"),
            KeyCode::Enter => f.write_str("enter"),
            KeyCode::Esc => f.write_str("esc"),
            KeyCode::Tab => f.write_str("tab"),
            KeyCode::BackTab => f.write_str("shift+tab"),
            KeyCode::Backspace => f.write_str("backspace"),
            KeyCode::Delete => f.write_str("delete"),
            KeyCode::Up => f.write_str("↑"),
            KeyCode::Down => f.write_str("↓"),
            KeyCode::Left => f.write_str("←"),
            KeyCode::Right => f.write_str("→"),
            KeyCode::Home => f.write_str("home"),
            KeyCode::End => f.write_str("end"),
            KeyCode::PageUp => f.write_str("pgup"),
            KeyCode::PageDown => f.write_str("pgdn"),
            KeyCode::F(n) => write!(f, "f{n}"),
            _ => f.write_str("?"),
        }
    }
}

/// The name to write into config.toml. Arrows print as words there.
pub fn config_name(key: Key) -> String {
    match key.code {
        KeyCode::Up => "up".into(),
        KeyCode::Down => "down".into(),
        KeyCode::Left => "left".into(),
        KeyCode::Right => "right".into(),
        KeyCode::BackTab => "backtab".into(),
        _ => key.to_string(),
    }
}

#[derive(Debug, Clone)]
pub struct Keymap {
    bindings: BTreeMap<Action, Vec<Key>>,
}

impl Keymap {
    pub fn defaults() -> Self {
        let bindings = Action::ALL
            .into_iter()
            .map(|action| {
                let keys = action
                    .default_keys()
                    .iter()
                    .filter_map(|k| k.parse().ok())
                    .collect();
                (action, keys)
            })
            .collect();
        Self { bindings }
    }

    /// Defaults with the user's overrides applied. Returns problems found in
    /// the config as readable messages; bad entries are skipped.
    pub fn from_config(overrides: &BTreeMap<String, Keys>) -> (Self, Vec<String>) {
        let mut keymap = Self::defaults();
        let mut problems = Vec::new();
        for (name, keys) in overrides {
            let Some(action) = Action::from_name(name) else {
                problems.push(format!("[keys] {name}: no such action"));
                continue;
            };
            let mut parsed = Vec::new();
            for text in keys.list() {
                match text.parse::<Key>() {
                    Ok(key) if key.is_reserved() => {
                        problems.push(format!("[keys] {name}: {text} is reserved"));
                    }
                    Ok(key) => parsed.push(key),
                    Err(err) => problems.push(format!("[keys] {name}: {err}")),
                }
            }
            // An empty list unbinds the action on purpose; a list where every
            // key was bad keeps the defaults.
            if !parsed.is_empty() || keys.list().is_empty() {
                keymap.bindings.insert(action, parsed);
            }
        }
        (keymap, problems)
    }

    pub fn action(&self, event: KeyEvent) -> Option<Action> {
        let pressed = Key::from_event(event);
        self.bindings
            .iter()
            .find(|(_, keys)| keys.contains(&pressed))
            .map(|(action, _)| *action)
    }

    pub fn keys(&self, action: Action) -> &[Key] {
        self.bindings.get(&action).map_or(&[], Vec::as_slice)
    }

    /// The first key of an action, for hints. Empty if it has none.
    pub fn first(&self, action: Action) -> String {
        self.keys(action)
            .first()
            .map(Key::to_string)
            .unwrap_or_default()
    }

    pub fn describe(&self, action: Action) -> String {
        let keys: Vec<String> = self.keys(action).iter().map(Key::to_string).collect();
        keys.join(" ")
    }

    /// Binds `key` to `action` alone, removing it from any other action.
    pub fn rebind(&mut self, action: Action, key: Key) {
        for keys in self.bindings.values_mut() {
            keys.retain(|k| *k != key);
        }
        self.bindings.insert(action, vec![key]);
    }

    pub fn reset(&mut self, action: Action) {
        let defaults = Self::defaults();
        self.bindings.insert(action, defaults.keys(action).to_vec());
    }

    /// Only the actions that differ from the defaults, for config.toml.
    pub fn overrides(&self) -> BTreeMap<String, Keys> {
        let defaults = Self::defaults();
        self.bindings
            .iter()
            .filter(|(action, keys)| defaults.keys(**action) != keys.as_slice())
            .map(|(action, keys)| {
                let names: Vec<String> = keys.iter().map(|k| config_name(*k)).collect();
                let value = match names.as_slice() {
                    [one] => Keys::One(one.clone()),
                    _ => Keys::Many(names),
                };
                (action.name().to_owned(), value)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    fn event(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    #[test]
    fn parses_and_prints_keys() {
        for text in [
            "s", "K", "?", "ctrl+s", "alt+x", "f5", "pgup", "space", "enter",
        ] {
            let key: Key = text.parse().unwrap();
            assert_eq!(key.to_string(), text);
        }
        assert_eq!("shift+tab".parse::<Key>().unwrap().to_string(), "shift+tab");
        assert!("ctrl+".parse::<Key>().is_err());
        assert!("hyper+x".parse::<Key>().is_err());
    }

    #[test]
    fn uppercase_letters_match_with_shift_held() {
        let keymap = Keymap::defaults();
        let shifted = event(KeyCode::Char('K'), KeyModifiers::SHIFT);
        assert_eq!(keymap.action(shifted), Some(Action::Kill));
        let plain = event(KeyCode::Char('k'), KeyModifiers::NONE);
        assert_eq!(keymap.action(plain), Some(Action::Up));
    }

    #[test]
    fn config_overrides_replace_only_the_named_actions() {
        let mut overrides = BTreeMap::new();
        overrides.insert("stop".to_string(), Keys::One("ctrl+s".into()));
        overrides.insert("nonsense".to_string(), Keys::One("z".into()));
        overrides.insert(
            "quit".to_string(),
            Keys::Many(vec!["esc".into(), "Q".into()]),
        );
        let (keymap, problems) = Keymap::from_config(&overrides);

        assert_eq!(keymap.describe(Action::Stop), "ctrl+s");
        assert_eq!(keymap.describe(Action::Open), "o");
        assert_eq!(keymap.describe(Action::Quit), "Q");
        assert_eq!(
            problems,
            vec![
                "[keys] nonsense: no such action".to_string(),
                "[keys] quit: esc is reserved".to_string(),
            ]
        );
    }

    #[test]
    fn rebinding_steals_the_key_and_round_trips_through_config() {
        let mut keymap = Keymap::defaults();
        keymap.rebind(Action::Open, "x".parse().unwrap());
        assert_eq!(keymap.describe(Action::Open), "x");
        assert_eq!(keymap.describe(Action::Stop), "");

        let (reloaded, problems) = Keymap::from_config(&keymap.overrides());
        assert!(problems.is_empty());
        assert_eq!(reloaded.describe(Action::Open), "x");

        keymap.reset(Action::Stop);
        assert_eq!(keymap.describe(Action::Stop), "x");
    }

    #[test]
    fn defaults_have_no_overrides() {
        assert!(Keymap::defaults().overrides().is_empty());
    }
}
