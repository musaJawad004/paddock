//! TUI state and the update function.
//!
//! `App::update` is the only place state changes. It takes a `Msg` (a key,
//! a timer tick, a backend event, the result of a side effect) and may
//! return an `Effect` for the event loop to carry out: send a request, copy
//! text, save the config. It never touches the terminal or the disk, so it
//! is tested directly. Overlays (help, settings, dialogs) handle their own
//! keys in `overlay.rs` and `settings.rs`.

use std::cell::Cell;
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::widgets::{Block, Paragraph, Widget};

use super::ansi;
use super::keys::{Action, Keymap};
use super::overlay::{Input, InputPurpose, Overlay, Picker};
use super::settings::Settings;
use super::theme::Theme;
use super::{details, help, logs_view, overlay, settings, sidebar, splash, status_bar};
use crate::config::Config;
use crate::ipc::protocol::{Event, Request};
use crate::model::{ListeningPort, ProcessId, ProcessInfo, ProcessState, Snapshot};

/// Lines kept per process on the TUI side. The backend keeps the full
/// scrollback; this is only what can be scrolled to without asking for more.
pub const LOG_LIMIT: usize = 5_000;
const MIN_WIDTH: u16 = 70;
const MIN_HEIGHT: u16 = 14;
const NOTICE_TIME: Duration = Duration::from_secs(6);

pub enum Msg {
    Key(KeyEvent),
    Resize,
    /// Sent by the frame timer; animations and clocks advance on it.
    Tick(Instant),
    Backend(Event),
    BackendGone,
    Copied {
        lines: usize,
    },
    Saved(Result<(), String>),
}

/// Work for the event loop. `App` decides, the loop does.
#[derive(Debug, PartialEq)]
pub enum Effect {
    Send(Request),
    Copy { text: String, lines: usize },
    Save(Config),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Processes,
    Ports,
    Logs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Logs,
    Details,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeKind {
    Info,
    Success,
    Error,
}

pub struct Notice {
    pub text: String,
    pub kind: NoticeKind,
    shown_at: Option<Instant>,
}

pub struct App {
    pub(super) theme: Theme,
    pub(super) keymap: Keymap,
    pub(super) config: Config,
    /// Where the data comes from, shown in the header ("demo data").
    pub(super) source: String,
    pub(super) snapshot: Snapshot,
    pub(super) logs: HashMap<ProcessId, VecDeque<String>>,
    /// Index into `processes()`, the flattened list across projects.
    pub(super) selected: usize,
    pub(super) selected_port: usize,
    pub(super) focus: Focus,
    pub(super) view: View,
    /// Lines scrolled up from the newest. 0 follows new output.
    pub(super) scroll: usize,
    /// Highlighted log line while the log pane has focus (index into logs).
    pub(super) cursor: Option<usize>,
    /// Other end of a selection started with `mark`.
    pub(super) mark: Option<usize>,
    pub(super) overlay: Option<Overlay>,
    pub(super) notice: Option<Notice>,
    pub(super) splash_started: Option<Instant>,
    pub(super) now: Instant,
    /// Log pane height from the last frame, for paging and keeping the
    /// cursor on screen.
    pub(super) log_height: Cell<usize>,
    dirty: bool,
    quit: bool,
}

impl App {
    /// Config mistakes (unknown theme, bad keys) are shown once at start-up.
    pub fn new(config: Config, source: impl Into<String>, splash: bool) -> Self {
        let (keymap, mut problems) = Keymap::from_config(&config.keys);
        if !Theme::exists(&config.ui.theme) {
            problems.push(format!("unknown theme \"{}\"", config.ui.theme));
        }
        let now = Instant::now();
        let notice = (!problems.is_empty()).then(|| Notice {
            text: format!("config.toml: {}", problems.join("; ")),
            kind: NoticeKind::Error,
            shown_at: None,
        });
        Self {
            theme: Theme::named(&config.ui.theme),
            keymap,
            config,
            source: source.into(),
            snapshot: Snapshot::default(),
            logs: HashMap::new(),
            selected: 0,
            selected_port: 0,
            focus: Focus::Processes,
            view: View::Logs,
            scroll: 0,
            cursor: None,
            mark: None,
            overlay: None,
            notice,
            splash_started: splash.then_some(now),
            now,
            log_height: Cell::new(20),
            dirty: true,
            quit: false,
        }
    }

    pub fn should_quit(&self) -> bool {
        self.quit
    }

    /// True once per change, so the loop draws only when something changed.
    pub fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }

    pub fn notify(&mut self, kind: NoticeKind, text: impl Into<String>) {
        self.notice = Some(Notice {
            text: text.into(),
            kind,
            shown_at: None,
        });
        self.dirty = true;
    }

    pub(super) fn splash_elapsed(&self) -> Option<Duration> {
        self.splash_started
            .map(|start| self.now.saturating_duration_since(start))
    }

    pub(super) fn processes(&self) -> impl Iterator<Item = &ProcessInfo> {
        self.snapshot.projects.iter().flat_map(|p| &p.processes)
    }

    pub(super) fn selected_process(&self) -> Option<&ProcessInfo> {
        self.processes().nth(self.selected)
    }

    pub(super) fn selected_logs(&self) -> Option<&VecDeque<String>> {
        self.logs.get(&self.selected_process()?.id)
    }

    fn selected_port(&self) -> Option<&ListeningPort> {
        self.snapshot.ports.get(self.selected_port)
    }

    pub fn update(&mut self, msg: Msg) -> Option<Effect> {
        if let Msg::Tick(now) = msg {
            self.on_tick(now);
            return None;
        }
        self.dirty = true;
        match msg {
            Msg::Key(key) => self.on_key(key),
            Msg::Tick(_) | Msg::Resize => None,
            Msg::Backend(event) => {
                self.on_event(event);
                None
            }
            Msg::BackendGone => {
                self.notify(NoticeKind::Error, "The backend stopped. Press q to quit.");
                None
            }
            Msg::Copied { lines } => {
                let what = if lines == 1 {
                    "1 line".to_owned()
                } else {
                    format!("{lines} lines")
                };
                self.notify(
                    NoticeKind::Success,
                    format!("Copied {what} to the clipboard."),
                );
                None
            }
            Msg::Saved(Ok(())) => None,
            Msg::Saved(Err(err)) => {
                self.notify(NoticeKind::Error, format!("Could not save settings: {err}"));
                None
            }
        }
    }

    fn on_tick(&mut self, now: Instant) {
        let second_changed = now.duration_since(self.now) >= Duration::from_secs(1);
        if second_changed || self.splash_started.is_some() {
            self.now = now;
        }
        if let Some(elapsed) = self.splash_elapsed() {
            self.dirty = true;
            if elapsed >= splash::DURATION {
                self.splash_started = None;
            }
        }
        if let Some(notice) = &mut self.notice {
            let shown_at = *notice.shown_at.get_or_insert(now);
            if now.duration_since(shown_at) >= NOTICE_TIME {
                self.notice = None;
                self.dirty = true;
            }
        }
        if second_changed && self.view == View::Details {
            self.dirty = true;
        }
    }

    fn on_key(&mut self, key: KeyEvent) -> Option<Effect> {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.quit = true;
            return None;
        }
        if self.splash_started.take().is_some() {
            return None;
        }
        if self.overlay.is_some() {
            return self.on_overlay_key(key);
        }
        self.notice = None;
        match key.code {
            KeyCode::Esc => {
                self.on_escape();
                return None;
            }
            KeyCode::Enter => return self.on_enter(),
            _ => {}
        }
        self.on_action(self.keymap.action(key)?)
    }

    fn on_escape(&mut self) {
        if self.mark.take().is_some() {
            return;
        }
        if self.focus != Focus::Processes {
            self.set_focus(Focus::Processes);
        }
    }

    fn on_enter(&mut self) -> Option<Effect> {
        match self.focus {
            Focus::Processes => self.set_focus(Focus::Logs),
            Focus::Ports => {
                let owner = self.selected_port()?.owner.clone()?;
                self.select_id(&owner);
                self.set_focus(Focus::Processes);
            }
            Focus::Logs => return self.copy_selection(),
        }
        None
    }

    fn on_action(&mut self, action: Action) -> Option<Effect> {
        match action {
            Action::Up => self.move_by(-1),
            Action::Down => self.move_by(1),
            Action::NextPane => self.cycle_focus(1),
            Action::PrevPane => self.cycle_focus(-1),
            Action::Start => return self.request_start(),
            Action::Stop => return self.request_stop(),
            Action::Restart => {
                let id = self.selected_process()?.id.clone();
                return Some(Effect::Send(Request::Restart(id)));
            }
            Action::Kill => self.confirm_kill(),
            Action::ChangePort => self.ask_port(),
            Action::MoveProject => self.pick_project(),
            Action::Details => {
                self.view = match self.view {
                    View::Logs => View::Details,
                    View::Details => View::Logs,
                };
            }
            Action::Mark => {
                if self.focus != Focus::Logs {
                    self.set_focus(Focus::Logs);
                }
                self.mark = match self.mark {
                    Some(_) => None,
                    None => self.cursor,
                };
            }
            Action::Copy => return self.copy_selection(),
            Action::CopyAll => return self.copy_all(),
            Action::ScrollUp => self.page(-1),
            Action::ScrollDown => self.page(1),
            Action::Follow => {
                self.scroll = 0;
                if self.cursor.is_some() {
                    self.cursor = self.last_line();
                }
            }
            Action::Settings => {
                self.overlay = Some(Overlay::Settings(Settings::new(self.theme.name())));
            }
            Action::Help => self.overlay = Some(Overlay::Help),
            Action::Quit => self.quit = true,
        }
        None
    }

    fn set_focus(&mut self, focus: Focus) {
        self.focus = focus;
        self.mark = None;
        if focus == Focus::Logs {
            self.view = View::Logs;
            self.cursor = self.visible_bottom();
        } else {
            self.cursor = None;
        }
    }

    fn cycle_focus(&mut self, step: isize) {
        const ORDER: [Focus; 3] = [Focus::Processes, Focus::Ports, Focus::Logs];
        let at = ORDER.iter().position(|f| *f == self.focus).unwrap_or(0);
        let next = (at as isize + step).rem_euclid(ORDER.len() as isize) as usize;
        self.set_focus(ORDER[next]);
    }

    fn move_by(&mut self, step: isize) {
        match self.focus {
            Focus::Processes => self.select(self.selected.saturating_add_signed(step)),
            Focus::Ports => {
                let last = self.snapshot.ports.len().saturating_sub(1);
                self.selected_port = self.selected_port.saturating_add_signed(step).min(last);
            }
            Focus::Logs => self.move_cursor(step),
        }
    }

    fn move_cursor(&mut self, step: isize) {
        let Some(last) = self.last_line() else {
            return;
        };
        let cursor = self.cursor.unwrap_or(last).saturating_add_signed(step);
        self.cursor = Some(cursor.min(last));
        self.keep_cursor_visible();
    }

    fn page(&mut self, direction: isize) {
        let page = self.log_height.get().saturating_sub(1).max(1);
        if self.focus == Focus::Logs {
            self.move_cursor(direction * page as isize);
            return;
        }
        let len = self.selected_logs().map_or(0, VecDeque::len);
        self.scroll = if direction < 0 {
            (self.scroll + page).min(len)
        } else {
            self.scroll.saturating_sub(page)
        };
    }

    fn last_line(&self) -> Option<usize> {
        self.selected_logs()?.len().checked_sub(1)
    }

    /// The newest line on screen: where the cursor starts.
    fn visible_bottom(&self) -> Option<usize> {
        let len = self.selected_logs()?.len();
        len.checked_sub(1 + self.scroll.min(len.saturating_sub(1)))
    }

    fn keep_cursor_visible(&mut self) {
        let (Some(cursor), Some(len)) = (self.cursor, self.selected_logs().map(VecDeque::len))
        else {
            return;
        };
        let height = self.log_height.get().max(1);
        let end = len - self.scroll.min(len);
        let start = end.saturating_sub(height);
        if cursor < start {
            self.scroll = len.saturating_sub(cursor + height);
        } else if cursor >= end {
            self.scroll = len - cursor - 1;
        }
    }

    /// First and last selected log line, inclusive.
    pub(super) fn selection(&self) -> Option<(usize, usize)> {
        let cursor = self.cursor?;
        let mark = self.mark.unwrap_or(cursor);
        Some((cursor.min(mark), cursor.max(mark)))
    }

    fn copy_selection(&mut self) -> Option<Effect> {
        let Some((from, to)) = self.selection() else {
            let tab = self.keymap.first(Action::NextPane);
            let all = self.keymap.first(Action::CopyAll);
            self.notify(
                NoticeKind::Info,
                format!("Press {tab} to pick lines in the logs, or {all} to copy them all."),
            );
            return None;
        };
        let logs = self.selected_logs()?;
        let text: Vec<String> = logs.range(from..=to).map(|l| ansi::strip(l)).collect();
        self.mark = None;
        Some(Effect::Copy {
            lines: text.len(),
            text: text.join("\n"),
        })
    }

    fn copy_all(&mut self) -> Option<Effect> {
        let Some(logs) = self.selected_logs().filter(|l| !l.is_empty()) else {
            self.notify(NoticeKind::Info, "Nothing to copy yet.");
            return None;
        };
        let text: Vec<String> = logs.iter().map(|l| ansi::strip(l)).collect();
        Some(Effect::Copy {
            lines: text.len(),
            text: text.join("\n"),
        })
    }

    fn select(&mut self, index: usize) {
        let last = self.processes().count().saturating_sub(1);
        let index = index.min(last);
        if index != self.selected {
            self.selected = index;
            self.scroll = 0;
            self.mark = None;
            if self.cursor.is_some() {
                self.cursor = self.last_line();
            }
        }
    }

    fn select_id(&mut self, id: &ProcessId) {
        let index = self.processes().position(|p| &p.id == id);
        if let Some(index) = index {
            self.select(index);
        }
    }

    fn request_start(&mut self) -> Option<Effect> {
        let process = self.selected_process()?;
        if process.state.is_up() || process.state == ProcessState::Stopping {
            let text = format!("{} is already {}.", process.id, process.state.label());
            self.notify(NoticeKind::Info, text);
            return None;
        }
        Some(Effect::Send(Request::Start(process.id.clone())))
    }

    fn request_stop(&mut self) -> Option<Effect> {
        let process = self.selected_process()?;
        if !process.state.is_up() {
            let text = format!("{} is not running.", process.id);
            self.notify(NoticeKind::Info, text);
            return None;
        }
        Some(Effect::Send(Request::Stop(process.id.clone())))
    }

    fn confirm_kill(&mut self) {
        if self.focus == Focus::Ports {
            let Some(port) = self.selected_port() else {
                return;
            };
            let (message, request) = match &port.owner {
                Some(owner) => (
                    format!(
                        "Kill {owner} (pid {}) listening on :{}?",
                        port.pid, port.port
                    ),
                    Request::Kill(owner.clone()),
                ),
                None => (
                    format!(
                        "Kill {} (pid {}) listening on :{}? Paddock did not start it.",
                        port.command, port.pid, port.port
                    ),
                    Request::KillPort {
                        port: port.port,
                        pid: port.pid,
                    },
                ),
            };
            self.overlay = Some(Overlay::Confirm { message, request });
            return;
        }
        let Some(process) = self.selected_process() else {
            return;
        };
        if !process.state.is_up() && process.state != ProcessState::Stopping {
            let text = format!("{} is not running.", process.id);
            self.notify(NoticeKind::Info, text);
            return;
        }
        let pid = process
            .pid
            .map(|p| format!(" (pid {p})"))
            .unwrap_or_default();
        self.overlay = Some(Overlay::Confirm {
            message: format!(
                "Kill {}{pid} right now? It gets no time to clean up.",
                process.id
            ),
            request: Request::Kill(process.id.clone()),
        });
    }

    fn ask_port(&mut self) {
        let Some(process) = self.selected_process() else {
            return;
        };
        self.overlay = Some(Overlay::Input(Input {
            title: format!("Port for {}", process.id),
            value: process.port.map(|p| p.to_string()).unwrap_or_default(),
            hint: "1 to 65535. A running process restarts on the new port.".into(),
            error: None,
            purpose: InputPurpose::Port(process.id.clone()),
        }));
    }

    fn pick_project(&mut self) {
        let Some(process) = self.selected_process() else {
            return;
        };
        let mut items: Vec<String> = self
            .snapshot
            .projects
            .iter()
            .map(|p| p.name.clone())
            .filter(|name| *name != process.id.project)
            .collect();
        items.push(overlay::NEW_PROJECT.into());
        self.overlay = Some(Overlay::Picker(Picker {
            title: format!("Move {} to", process.id),
            items,
            selected: 0,
            target: process.id.clone(),
        }));
    }

    /// The config as it should be saved: current theme and keys.
    pub(super) fn config_to_save(&self) -> Config {
        let mut config = self.config.clone();
        config.ui.theme = self.theme.name().to_owned();
        config.keys = self.keymap.overrides();
        config
    }

    fn on_event(&mut self, event: Event) {
        match event {
            Event::Snapshot(snapshot) => {
                let selected = self.selected_process().map(|p| p.id.clone());
                self.snapshot = snapshot;
                if let Some(index) =
                    selected.and_then(|id| self.processes().position(|p| p.id == id))
                {
                    self.selected = index;
                }
                self.select(self.selected);
                self.clamp_port_selection();
            }
            Event::State { id, state } => {
                if let Some(process) = self.process_mut(&id) {
                    process.state = state;
                    if !state.is_up() {
                        process.usage = None;
                    }
                }
            }
            Event::Output { id, lines } => self.append_output(id, lines),
            Event::Ports(ports) => {
                self.snapshot.ports = ports;
                self.clamp_port_selection();
            }
            Event::Usage(usage) => {
                for (id, usage) in usage {
                    if let Some(process) = self.process_mut(&id) {
                        process.usage = Some(usage);
                    }
                }
            }
            Event::Renamed { from, to } => {
                if let Some(logs) = self.logs.remove(&from) {
                    self.logs.insert(to, logs);
                }
            }
            Event::Notice(text) => self.notify(NoticeKind::Info, text),
        }
    }

    fn clamp_port_selection(&mut self) {
        self.selected_port = self
            .selected_port
            .min(self.snapshot.ports.len().saturating_sub(1));
    }

    fn append_output(&mut self, id: ProcessId, lines: Vec<String>) {
        let added = lines.len();
        let is_selected = self.selected_process().is_some_and(|p| p.id == id);
        let buffer = self.logs.entry(id).or_default();
        buffer.extend(lines);
        let overflow = buffer.len().saturating_sub(LOG_LIMIT);
        buffer.drain(..overflow);
        let len = buffer.len();
        if !is_selected {
            return;
        }
        // Keep the view still while new lines arrive below it, and keep the
        // cursor and mark on the same lines when old ones are dropped.
        if self.scroll > 0 || self.cursor.is_some() {
            self.scroll = (self.scroll + added).min(len);
        }
        self.cursor = self.cursor.map(|c| c.saturating_sub(overflow));
        self.mark = self.mark.map(|m| m.saturating_sub(overflow));
    }

    fn process_mut(&mut self, id: &ProcessId) -> Option<&mut ProcessInfo> {
        self.snapshot
            .projects
            .iter_mut()
            .flat_map(|p| &mut p.processes)
            .find(|p| &p.id == id)
    }
}

impl Widget for &App {
    fn render(self, area: Rect, buf: &mut Buffer) {
        Block::new().style(self.theme.base()).render(area, buf);
        if let Some(elapsed) = self.splash_elapsed() {
            splash::render(self.theme, elapsed, area, buf);
            return;
        }
        if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
            let message = format!(
                "Terminal too small ({}x{}). Paddock needs at least {MIN_WIDTH}x{MIN_HEIGHT}.",
                area.width, area.height
            );
            Paragraph::new(message)
                .style(self.theme.warning())
                .render(area, buf);
            return;
        }

        let [header, body, footer] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Fill(1),
            Constraint::Length(1),
        ])
        .areas(area);
        let [side, main] =
            Layout::horizontal([Constraint::Length(sidebar::WIDTH), Constraint::Fill(1)])
                .areas(body);
        let ports_height = (self.snapshot.ports.len() as u16 + 2).clamp(3, 9);
        let [projects, ports] =
            Layout::vertical([Constraint::Fill(1), Constraint::Length(ports_height)]).areas(side);

        status_bar::render_header(self, header, buf);
        sidebar::render_projects(self, projects, buf);
        sidebar::render_ports(self, ports, buf);
        match self.view {
            View::Logs => logs_view::render(self, main, buf),
            View::Details => details::render(self, main, buf),
        }
        status_bar::render_footer(self, footer, buf);

        match &self.overlay {
            Some(Overlay::Help) => help::render(self, area, buf),
            Some(Overlay::Settings(state)) => settings::render(self, state, area, buf),
            Some(dialog) => overlay::render(self, dialog, area, buf),
            None => {}
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::path::PathBuf;

    use crossterm::event::KeyModifiers;
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::model::ProjectInfo;

    pub(crate) fn process(project: &str, name: &str, state: ProcessState) -> ProcessInfo {
        ProcessInfo {
            id: ProcessId::new(project, name),
            command: format!("run {name}"),
            state,
            port: None,
            usage: None,
            cwd: PathBuf::from(format!("/tmp/{project}")),
            source: "test fixture".into(),
            pid: state.is_up().then_some(4242),
            started_at_ms: None,
            restarts: 0,
        }
    }

    pub(crate) fn app() -> App {
        let mut app = App::new(Config::default(), "test", false);
        app.theme = Theme::plain();
        let snapshot = Snapshot {
            projects: vec![
                ProjectInfo {
                    name: "shop".into(),
                    path: PathBuf::from("/tmp/shop"),
                    processes: vec![
                        process("shop", "web", ProcessState::Running),
                        process("shop", "api", ProcessState::Stopped),
                    ],
                },
                ProjectInfo {
                    name: "docs".into(),
                    path: PathBuf::from("/tmp/docs"),
                    processes: vec![process("docs", "dev", ProcessState::Crashed(Some(1)))],
                },
            ],
            ports: vec![
                ListeningPort {
                    port: 3000,
                    pid: 4242,
                    command: "node".into(),
                    owner: Some(ProcessId::new("shop", "web")),
                },
                ListeningPort {
                    port: 5000,
                    pid: 812,
                    command: "ControlCenter".into(),
                    owner: None,
                },
            ],
        };
        app.update(Msg::Backend(Event::Snapshot(snapshot)));
        app
    }

    pub(crate) fn press(app: &mut App, code: KeyCode) -> Option<Effect> {
        app.update(Msg::Key(KeyEvent::new(code, KeyModifiers::NONE)))
    }

    fn output(app: &mut App, project: &str, name: &str, count: usize) {
        let lines = (0..count).map(|i| format!("line {i}")).collect();
        app.update(Msg::Backend(Event::Output {
            id: ProcessId::new(project, name),
            lines,
        }));
    }

    fn send(request: Request) -> Option<Effect> {
        Some(Effect::Send(request))
    }

    #[test]
    fn selection_moves_across_projects_and_stops_at_the_ends() {
        let mut app = app();
        press(&mut app, KeyCode::Up);
        assert_eq!(app.selected, 0);
        for _ in 0..5 {
            press(&mut app, KeyCode::Down);
        }
        assert_eq!(
            app.selected_process().map(|p| p.id.to_string()),
            Some("docs/dev".into())
        );
    }

    #[test]
    fn start_and_stop_are_sent_only_when_they_make_sense() {
        let mut app = app();
        assert_eq!(press(&mut app, KeyCode::Char('s')), None);
        assert_eq!(
            app.notice.as_ref().map(|n| n.text.as_str()),
            Some("shop/web is already running.")
        );
        assert_eq!(
            press(&mut app, KeyCode::Char('x')),
            send(Request::Stop(ProcessId::new("shop", "web")))
        );
        press(&mut app, KeyCode::Down);
        assert_eq!(
            press(&mut app, KeyCode::Char('s')),
            send(Request::Start(ProcessId::new("shop", "api")))
        );
        assert_eq!(press(&mut app, KeyCode::Char('x')), None);
    }

    #[test]
    fn kill_asks_first() {
        let mut app = app();
        assert_eq!(press(&mut app, KeyCode::Char('K')), None);
        assert!(matches!(app.overlay, Some(Overlay::Confirm { .. })));
        assert_eq!(
            press(&mut app, KeyCode::Char('y')),
            send(Request::Kill(ProcessId::new("shop", "web")))
        );
        assert!(app.overlay.is_none());

        press(&mut app, KeyCode::Char('K'));
        assert_eq!(press(&mut app, KeyCode::Char('n')), None);
        assert!(app.overlay.is_none());
    }

    #[test]
    fn killing_a_foreign_port_targets_its_pid() {
        let mut app = app();
        press(&mut app, KeyCode::Tab);
        assert_eq!(app.focus, Focus::Ports);
        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Char('K'));
        assert_eq!(
            press(&mut app, KeyCode::Enter),
            send(Request::KillPort {
                port: 5000,
                pid: 812
            })
        );
    }

    #[test]
    fn enter_on_an_owned_port_jumps_to_its_process() {
        let mut app = app();
        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Tab);
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.focus, Focus::Processes);
        assert_eq!(
            app.selected_process().map(|p| p.id.to_string()),
            Some("shop/web".into())
        );
    }

    #[test]
    fn change_port_validates_input() {
        let mut app = app();
        press(&mut app, KeyCode::Char('p'));
        for c in "99999".chars() {
            press(&mut app, KeyCode::Char(c));
        }
        assert_eq!(press(&mut app, KeyCode::Enter), None);
        let Some(Overlay::Input(input)) = &app.overlay else {
            panic!("input closed on a bad port");
        };
        assert!(input.error.is_some());

        for _ in 0..5 {
            press(&mut app, KeyCode::Backspace);
        }
        for c in "3100".chars() {
            press(&mut app, KeyCode::Char(c));
        }
        assert_eq!(
            press(&mut app, KeyCode::Enter),
            send(Request::SetPort {
                id: ProcessId::new("shop", "web"),
                port: 3100
            })
        );
    }

    #[test]
    fn move_to_an_existing_or_new_project() {
        let mut app = app();
        press(&mut app, KeyCode::Char('m'));
        assert_eq!(
            press(&mut app, KeyCode::Enter),
            send(Request::Move {
                id: ProcessId::new("shop", "web"),
                project: "docs".into()
            })
        );

        press(&mut app, KeyCode::Char('m'));
        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Enter);
        for c in "tools".chars() {
            press(&mut app, KeyCode::Char(c));
        }
        assert_eq!(
            press(&mut app, KeyCode::Enter),
            send(Request::Move {
                id: ProcessId::new("shop", "web"),
                project: "tools".into()
            })
        );
    }

    #[test]
    fn copy_a_line_a_range_and_everything() {
        let mut app = app();
        app.log_height.set(5);
        app.update(Msg::Backend(Event::Output {
            id: ProcessId::new("shop", "web"),
            lines: vec![
                "one".into(),
                "\u{1b}[32mtwo\u{1b}[0m".into(),
                "three".into(),
            ],
        }));

        press(&mut app, KeyCode::Tab);
        press(&mut app, KeyCode::Tab);
        assert_eq!(app.focus, Focus::Logs);
        assert_eq!(app.cursor, Some(2));
        assert_eq!(
            press(&mut app, KeyCode::Char('y')),
            Some(Effect::Copy {
                text: "three".into(),
                lines: 1
            })
        );

        press(&mut app, KeyCode::Char('v'));
        press(&mut app, KeyCode::Up);
        press(&mut app, KeyCode::Up);
        assert_eq!(
            press(&mut app, KeyCode::Char('y')),
            Some(Effect::Copy {
                text: "one\ntwo\nthree".into(),
                lines: 3
            })
        );

        press(&mut app, KeyCode::Esc);
        assert_eq!(app.focus, Focus::Processes);
        assert_eq!(
            press(&mut app, KeyCode::Char('Y')),
            Some(Effect::Copy {
                text: "one\ntwo\nthree".into(),
                lines: 3
            })
        );
    }

    #[test]
    fn cursor_moves_scroll_the_view() {
        let mut app = app();
        app.log_height.set(5);
        output(&mut app, "shop", "web", 50);
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.cursor, Some(49));
        for _ in 0..10 {
            press(&mut app, KeyCode::Up);
        }
        assert_eq!(app.cursor, Some(39));
        assert_eq!(app.scroll, 6);
        press(&mut app, KeyCode::Char('G'));
        assert_eq!((app.cursor, app.scroll), (Some(49), 0));
    }

    #[test]
    fn logs_are_capped_and_the_cursor_follows_its_line() {
        let mut app = app();
        output(&mut app, "shop", "web", LOG_LIMIT);
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Up);
        assert_eq!(app.cursor, Some(LOG_LIMIT - 2));
        output(&mut app, "shop", "web", 10);
        assert_eq!(app.selected_logs().map(VecDeque::len), Some(LOG_LIMIT));
        assert_eq!(app.cursor, Some(LOG_LIMIT - 12));
    }

    #[test]
    fn renamed_processes_keep_their_logs() {
        let mut app = app();
        output(&mut app, "shop", "web", 3);
        let to = ProcessId::new("docs", "web");
        app.update(Msg::Backend(Event::Renamed {
            from: ProcessId::new("shop", "web"),
            to: to.clone(),
        }));
        assert_eq!(app.logs.get(&to).map(VecDeque::len), Some(3));
    }

    #[test]
    fn rebinding_a_key_in_settings_saves_the_config() {
        let mut app = app();
        press(&mut app, KeyCode::Char(','));
        press(&mut app, KeyCode::Tab);
        // Keys tab; the first row is "up". Rebind it to w.
        press(&mut app, KeyCode::Enter);
        let effect = press(&mut app, KeyCode::Char('w'));
        let Some(Effect::Save(config)) = effect else {
            panic!("expected a save, got {effect:?}");
        };
        assert_eq!(config.keys.len(), 1);
        assert_eq!(app.keymap.describe(Action::Up), "w");
    }

    #[test]
    fn picking_a_theme_previews_and_saves_it() {
        let mut app = app();
        press(&mut app, KeyCode::Char(','));
        press(&mut app, KeyCode::Down);
        assert_eq!(app.theme.name(), "terminal");
        let Some(Effect::Save(config)) = press(&mut app, KeyCode::Enter) else {
            panic!("expected a save");
        };
        assert_eq!(config.ui.theme, "terminal");

        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Esc);
        assert_eq!(
            app.theme.name(),
            "terminal",
            "Esc reverts an unsaved preview"
        );
    }

    #[test]
    fn any_key_skips_the_splash_and_ctrl_c_always_quits() {
        let mut app = App::new(Config::default(), "test", true);
        assert!(app.splash_elapsed().is_some());
        press(&mut app, KeyCode::Char('z'));
        assert!(app.splash_elapsed().is_none());
        app.update(Msg::Key(KeyEvent::new(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL,
        )));
        assert!(app.should_quit());
    }

    #[test]
    fn config_problems_are_shown_at_start() {
        let mut config = Config::default();
        config.ui.theme = "neon".into();
        let app = App::new(config, "test", false);
        let notice = app
            .notice
            .as_ref()
            .map(|n| n.text.clone())
            .unwrap_or_default();
        assert!(notice.contains("unknown theme \"neon\""), "{notice}");
    }

    fn render(app: &App, width: u16, height: u16) -> ratatui::backend::TestBackend {
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
        terminal.draw(|f| f.render_widget(app, f.area())).unwrap();
        terminal.backend().clone()
    }

    #[test]
    fn renders_the_dashboard() {
        let mut app = app();
        output(&mut app, "shop", "web", 30);
        app.update(Msg::Backend(Event::Output {
            id: ProcessId::new("shop", "web"),
            lines: vec!["Error: something broke".into()],
        }));
        insta::assert_snapshot!(render(&app, 100, 22));
    }

    #[test]
    fn renders_details() {
        let mut app = app();
        press(&mut app, KeyCode::Char('i'));
        insta::assert_snapshot!(render(&app, 100, 22));
    }

    #[test]
    fn renders_help() {
        let mut app = app();
        press(&mut app, KeyCode::Char('?'));
        insta::assert_snapshot!(render(&app, 100, 36));
    }

    #[test]
    fn renders_settings() {
        let mut app = app();
        press(&mut app, KeyCode::Char(','));
        insta::assert_snapshot!(render(&app, 100, 24));
    }

    #[test]
    fn small_terminals_get_a_message_instead_of_a_broken_layout() {
        let backend = render(&app(), 40, 10);
        assert!(format!("{backend}").contains("Terminal too small"));
    }
}
