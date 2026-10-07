//! TUI state and the update function.
//!
//! `App::update` is the only place state changes. It takes a `Msg` (a key,
//! a timer tick, a monitor event, the result of a side effect) and may
//! return an `Effect` for the event loop to carry out: send a request, copy
//! text, open a URL, save the config. It never touches the terminal or the
//! disk, so it is tested directly. Popups handle their own keys in
//! `overlay.rs` and `settings.rs`.
//!
//! The selected server is the followed one: whenever the selection changes,
//! `take_follow` hands the loop a `Request::Follow` so its log streams in.

use std::cell::Cell;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::widgets::{Block, Paragraph, Widget};

use super::ansi;
use super::keys::{Action, Keymap};
use super::overlay::Overlay;
use super::settings::Settings;
use super::theme::Theme;
use super::{details, help, logs_view, overlay, settings, sidebar, splash, status_bar};
use crate::config::Config;
use crate::ipc::protocol::{Event, Request};
use crate::model::{ListeningPort, Server, ServerId, ServerState, Snapshot};

/// Log lines kept for the followed server.
pub const LOG_LIMIT: usize = 5_000;
const MIN_WIDTH: u16 = 70;
const MIN_HEIGHT: u16 = 18;
const NOTICE_TIME: Duration = Duration::from_secs(6);

pub enum Msg {
    Key(KeyEvent),
    Resize,
    /// Sent by the frame timer; animations and clocks advance on it.
    Tick(Instant),
    Monitor(Event),
    MonitorGone,
    Copied(String),
    Saved(Result<(), String>),
}

/// Work for the event loop. `App` decides, the loop does.
#[derive(Debug, PartialEq)]
pub enum Effect {
    Send(Request),
    /// `what` names it in the notice, e.g. "the URL".
    Copy {
        text: String,
        what: String,
    },
    Open(String),
    Save(Config),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Servers,
    Logs,
    Ports,
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
    /// Where the data comes from, shown in the header ("local", "demo data").
    pub(super) source: String,
    pub(super) snapshot: Snapshot,
    /// Followed across scans by id, not by position.
    selected: Option<ServerId>,
    /// The server whose log the monitor streams to us.
    followed: Option<ServerId>,
    pub(super) logs: VecDeque<String>,
    /// Lines scrolled up from the newest. 0 follows new output.
    pub(super) scroll: usize,
    /// Highlighted log line while the log pane has focus (index into logs).
    pub(super) cursor: Option<usize>,
    /// Other end of a selection started with `mark`.
    pub(super) mark: Option<usize>,
    /// Log pane height from the last frame, for paging.
    pub(super) log_height: Cell<usize>,
    pub(super) selected_port: usize,
    pub(super) focus: Focus,
    pub(super) overlay: Option<Overlay>,
    pub(super) notice: Option<Notice>,
    pub(super) splash_started: Option<Instant>,
    pub(super) now: Instant,
    /// False until the first scan arrives, so the empty state is not shown
    /// while Paddock is still looking.
    pub(super) scanned: bool,
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
            selected: None,
            followed: None,
            logs: VecDeque::new(),
            scroll: 0,
            cursor: None,
            mark: None,
            log_height: Cell::new(10),
            selected_port: 0,
            focus: Focus::Servers,
            overlay: None,
            notice,
            splash_started: splash.then_some(now),
            now,
            scanned: false,
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

    /// A `Follow` request when the selected server changed since last time.
    pub fn take_follow(&mut self) -> Option<Request> {
        if self.followed == self.selected {
            return None;
        }
        self.followed = self.selected;
        self.logs.clear();
        self.scroll = 0;
        self.cursor = None;
        self.mark = None;
        if self.focus == Focus::Logs {
            self.focus = Focus::Servers;
        }
        Some(Request::Follow(self.selected))
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

    pub(super) fn selected_server(&self) -> Option<&Server> {
        let id = self.selected?;
        self.snapshot.servers().find(|s| s.id == id)
    }

    fn selected_index(&self) -> Option<usize> {
        let id = self.selected?;
        self.snapshot.servers().position(|s| s.id == id)
    }

    fn selected_listener(&self) -> Option<&ListeningPort> {
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
            Msg::Monitor(event) => {
                self.on_event(event);
                None
            }
            Msg::MonitorGone => {
                self.notify(NoticeKind::Error, "The monitor stopped. Press q to quit.");
                None
            }
            Msg::Copied(what) => {
                self.notify(NoticeKind::Success, format!("Copied {what}."));
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
        // Uptime in the details pane.
        if second_changed && self.selected.is_some() {
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
                if self.mark.take().is_none() {
                    self.set_focus(Focus::Servers);
                }
                return None;
            }
            KeyCode::Enter => return self.on_enter(),
            _ => {}
        }
        self.on_action(self.keymap.action(key)?)
    }

    fn on_enter(&mut self) -> Option<Effect> {
        match self.focus {
            Focus::Servers => self.set_focus(Focus::Logs),
            Focus::Logs => return self.copy_lines(),
            Focus::Ports => {
                let owner = self.selected_listener()?.owner;
                self.selected = Some(owner);
                self.set_focus(Focus::Servers);
            }
        }
        None
    }

    fn on_action(&mut self, action: Action) -> Option<Effect> {
        match action {
            Action::Up => self.move_by(-1),
            Action::Down => self.move_by(1),
            Action::NextPane => self.cycle_focus(1),
            Action::PrevPane => self.cycle_focus(-1),
            Action::Open => return self.open(),
            Action::Copy => {
                return match self.focus {
                    Focus::Logs => self.copy_lines(),
                    _ => self.copy_url(),
                };
            }
            Action::CopyAll => {
                // A server without a log: Y copies the command that gives it
                // one, which is what the screen offers.
                let without_log = self.selected_server().is_some_and(|s| s.log.is_none());
                return match self.focus {
                    Focus::Logs => self.copy_all_logs(),
                    Focus::Servers if without_log => self.copy_run_command(),
                    _ => self.copy_command(),
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
            Action::ScrollUp => self.page(-1),
            Action::ScrollDown => self.page(1),
            Action::Follow => {
                self.scroll = 0;
                if self.cursor.is_some() {
                    self.cursor = self.logs.len().checked_sub(1);
                }
            }
            Action::Stop => self.confirm_stop(false),
            Action::Kill => self.confirm_stop(true),
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
        self.cursor = match focus {
            Focus::Logs => self.visible_bottom(),
            _ => None,
        };
    }

    fn cycle_focus(&mut self, step: isize) {
        const ORDER: [Focus; 3] = [Focus::Servers, Focus::Logs, Focus::Ports];
        let at = ORDER.iter().position(|f| *f == self.focus).unwrap_or(0);
        let next = (at as isize + step).rem_euclid(ORDER.len() as isize) as usize;
        self.set_focus(ORDER[next]);
    }

    fn move_by(&mut self, step: isize) {
        match self.focus {
            Focus::Servers => {
                let ids: Vec<ServerId> = self.snapshot.servers().map(|s| s.id).collect();
                let Some(last) = ids.len().checked_sub(1) else {
                    return;
                };
                let at = self.selected_index().unwrap_or(0);
                self.selected = Some(ids[at.saturating_add_signed(step).min(last)]);
            }
            Focus::Logs => self.move_cursor(step),
            Focus::Ports => {
                let last = self.snapshot.ports.len().saturating_sub(1);
                self.selected_port = self.selected_port.saturating_add_signed(step).min(last);
            }
        }
    }

    fn move_cursor(&mut self, step: isize) {
        let Some(last) = self.logs.len().checked_sub(1) else {
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
        self.scroll = if direction < 0 {
            (self.scroll + page).min(self.logs.len())
        } else {
            self.scroll.saturating_sub(page)
        };
    }

    /// The newest line on screen: where the cursor starts.
    fn visible_bottom(&self) -> Option<usize> {
        let len = self.logs.len();
        len.checked_sub(1 + self.scroll.min(len.saturating_sub(1)))
    }

    fn keep_cursor_visible(&mut self) {
        let Some(cursor) = self.cursor else {
            return;
        };
        let len = self.logs.len();
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

    /// The port the user is pointing at: the selected row in the Ports pane,
    /// else the selected server's first port.
    fn target_port(&self) -> Option<u16> {
        match self.focus {
            Focus::Ports => self.selected_listener().map(|l| l.port),
            _ => self.selected_server()?.ports.first().copied(),
        }
    }

    fn open(&mut self) -> Option<Effect> {
        let Some(port) = self.target_port() else {
            self.notify(NoticeKind::Info, "Nothing selected to open.");
            return None;
        };
        Some(Effect::Open(format!("http://localhost:{port}")))
    }

    fn copy_url(&mut self) -> Option<Effect> {
        let port = self.target_port()?;
        Some(Effect::Copy {
            text: format!("http://localhost:{port}"),
            what: "the URL".into(),
        })
    }

    fn copy_command(&mut self) -> Option<Effect> {
        let server = self.selected_server()?;
        Some(Effect::Copy {
            text: server.command.clone(),
            what: "the command".into(),
        })
    }

    fn copy_lines(&mut self) -> Option<Effect> {
        if self.selected_server().is_some_and(|s| s.log.is_none()) {
            return self.copy_run_command();
        }
        let (from, to) = self.selection()?;
        let text: Vec<String> = self.logs.range(from..=to).map(|l| ansi::strip(l)).collect();
        self.mark = None;
        let what = if text.len() == 1 {
            "1 line".to_owned()
        } else {
            format!("{} lines", text.len())
        };
        Some(Effect::Copy {
            text: text.join("\n"),
            what,
        })
    }

    fn copy_all_logs(&mut self) -> Option<Effect> {
        if self.selected_server().is_some_and(|s| s.log.is_none()) {
            return self.copy_run_command();
        }
        if self.logs.is_empty() {
            self.notify(NoticeKind::Info, "No log lines yet.");
            return None;
        }
        let text: Vec<String> = self.logs.iter().map(|l| ansi::strip(l)).collect();
        Some(Effect::Copy {
            what: format!("{} lines", text.len()),
            text: text.join("\n"),
        })
    }

    /// For a server without a log: the command that restarts it with one.
    fn copy_run_command(&mut self) -> Option<Effect> {
        let server = self.selected_server()?;
        Some(Effect::Copy {
            text: run_command(server),
            what: "the restart command".into(),
        })
    }

    fn confirm_stop(&mut self, force: bool) {
        let target = match self.focus {
            Focus::Ports => self.selected_listener().map(|l| l.owner),
            _ => self.selected_server().map(|s| s.id),
        };
        let Some(server) = target.and_then(|id| self.snapshot.servers().find(|s| s.id == id))
        else {
            self.notify(NoticeKind::Info, "No server selected.");
            return;
        };
        let label = self.server_label(server.id);
        let processes = if server.processes == 1 {
            "its process".to_owned()
        } else {
            format!("its {} processes", server.processes)
        };
        let (message, request) = if force {
            (
                format!("Kill {label} and {processes} right now? They get no time to clean up."),
                Request::Kill(server.id),
            )
        } else {
            (
                format!(
                    "Stop {label}? Paddock sends SIGTERM to {processes}, then SIGKILL after 5 s."
                ),
                Request::Stop(server.id),
            )
        };
        self.overlay = Some(Overlay::Confirm { message, request });
    }

    /// "verbatim › dev".
    pub(super) fn server_label(&self, id: ServerId) -> String {
        self.snapshot
            .projects
            .iter()
            .find_map(|p| {
                let server = p.servers.iter().find(|s| s.id == id)?;
                Some(format!("{} › {}", p.name, server.name))
            })
            .unwrap_or_else(|| format!("pid {}", id.pid))
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
                self.snapshot = snapshot;
                self.scanned = true;
                if self.selected_server().is_none() {
                    self.selected = self.snapshot.servers().next().map(|s| s.id);
                }
                self.selected_port = self
                    .selected_port
                    .min(self.snapshot.ports.len().saturating_sub(1));
            }
            Event::Logs { id, lines, reset } => {
                if Some(id) != self.followed {
                    return;
                }
                if reset {
                    self.logs.clear();
                    self.scroll = 0;
                    self.cursor = None;
                    self.mark = None;
                }
                self.append_logs(lines);
            }
            Event::Notice(text) => self.notify(NoticeKind::Info, text),
        }
    }

    fn append_logs(&mut self, lines: Vec<String>) {
        let added = lines.len();
        self.logs.extend(lines);
        let overflow = self.logs.len().saturating_sub(LOG_LIMIT);
        self.logs.drain(..overflow);
        // Keep a scrolled or selected view still while new lines arrive.
        if self.scroll > 0 || self.cursor.is_some() {
            self.scroll = (self.scroll + added).min(self.logs.len());
        }
        self.cursor = self.cursor.map(|c| c.saturating_sub(overflow));
        self.mark = self.mark.map(|m| m.saturating_sub(overflow));
    }

    pub(super) fn is_selected(&self, id: ServerId) -> bool {
        self.selected == Some(id)
    }

    pub(super) fn counts(&self) -> (usize, usize) {
        let running = self
            .snapshot
            .servers()
            .filter(|s| s.state == ServerState::Running)
            .count();
        let stopping = self.snapshot.servers().count() - running;
        (running, stopping)
    }
}

/// One line to paste in a terminal: go to the server's folder and start the
/// same command with its log captured.
pub(super) fn run_command(server: &Server) -> String {
    let folder = super::tilde(&server.cwd);
    let folder = if folder.contains(' ') {
        format!("\"{folder}\"")
    } else {
        folder
    };
    format!("cd {folder} && paddock run {}", server.command)
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
        let ports_height = (self.snapshot.ports.len() as u16 + 2).clamp(3, 10);
        let [servers, ports] =
            Layout::vertical([Constraint::Fill(1), Constraint::Length(ports_height)]).areas(side);

        status_bar::render_header(self, header, buf);
        sidebar::render_servers(self, servers, buf);
        sidebar::render_ports(self, ports, buf);
        if self.selected_server().is_some() {
            let [info, logs] =
                Layout::vertical([Constraint::Length(details::HEIGHT), Constraint::Fill(1)])
                    .areas(main);
            details::render(self, info, buf);
            logs_view::render(self, logs, buf);
        } else {
            details::render(self, main, buf);
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
    use crate::model::{Project, ResourceUsage};

    /// Started "in the future", so uptime shows 0s and snapshots never
    /// depend on today's date.
    fn id(pid: u32) -> ServerId {
        ServerId {
            pid,
            started: u64::MAX / 2,
        }
    }

    fn server(pid: u32, name: &str, ports: &[u16], log: bool) -> Server {
        Server {
            id: id(pid),
            name: name.into(),
            command: format!("npm run {name}"),
            cwd: PathBuf::from("/home/me/shop"),
            ports: ports.to_vec(),
            processes: 3,
            usage: ResourceUsage {
                cpu_percent: 2.5,
                memory_bytes: 120 * 1024 * 1024,
            },
            state: ServerState::Running,
            log: log.then(|| PathBuf::from(format!("/logs/{pid}.log"))),
        }
    }

    fn snapshot() -> Snapshot {
        let web = server(100, "dev", &[3000], true);
        Snapshot {
            projects: vec![
                Project {
                    name: "shop".into(),
                    path: PathBuf::from("/home/me/shop"),
                    kind: "node".into(),
                    servers: vec![web.clone(), server(200, "api", &[4000], false)],
                },
                Project {
                    name: "docs".into(),
                    path: PathBuf::from("/home/me/docs"),
                    kind: "folder".into(),
                    servers: vec![server(300, "http.server", &[8000], false)],
                },
            ],
            ports: vec![
                ListeningPort {
                    port: 3000,
                    pid: 101,
                    command: "node".into(),
                    owner: web.id,
                },
                ListeningPort {
                    port: 4000,
                    pid: 201,
                    command: "node".into(),
                    owner: id(200),
                },
            ],
        }
    }

    pub(crate) fn app() -> App {
        let mut app = App::new(Config::default(), "test", false);
        app.theme = Theme::plain();
        app.update(Msg::Monitor(Event::Snapshot(snapshot())));
        assert_eq!(app.take_follow(), Some(Request::Follow(Some(id(100)))));
        app
    }

    pub(crate) fn press(app: &mut App, code: KeyCode) -> Option<Effect> {
        app.update(Msg::Key(KeyEvent::new(code, KeyModifiers::NONE)))
    }

    fn send(request: Request) -> Option<Effect> {
        Some(Effect::Send(request))
    }

    fn logs(app: &mut App, lines: &[&str], reset: bool) {
        app.update(Msg::Monitor(Event::Logs {
            id: id(100),
            lines: lines.iter().map(|l| l.to_string()).collect(),
            reset,
        }));
    }

    fn copied(text: &str, what: &str) -> Option<Effect> {
        Some(Effect::Copy {
            text: text.into(),
            what: what.into(),
        })
    }

    #[test]
    fn selection_moves_across_projects_and_stops_at_the_ends() {
        let mut app = app();
        press(&mut app, KeyCode::Up);
        assert_eq!(app.selected_server().map(|s| s.id), Some(id(100)));
        for _ in 0..5 {
            press(&mut app, KeyCode::Down);
        }
        assert_eq!(app.selected_server().map(|s| s.id), Some(id(300)));
    }

    #[test]
    fn changing_the_selection_follows_the_new_server_once() {
        let mut app = app();
        logs(&mut app, &["old"], true);
        press(&mut app, KeyCode::Down);
        assert_eq!(app.take_follow(), Some(Request::Follow(Some(id(200)))));
        assert!(app.logs.is_empty(), "the old server's lines are gone");
        assert_eq!(app.take_follow(), None);
    }

    #[test]
    fn logs_for_another_server_are_ignored() {
        let mut app = app();
        app.update(Msg::Monitor(Event::Logs {
            id: id(999),
            lines: vec!["stray".into()],
            reset: true,
        }));
        assert!(app.logs.is_empty());
    }

    #[test]
    fn selection_falls_back_when_the_server_goes() {
        let mut app = app();
        let mut gone = snapshot();
        gone.projects[0].servers.remove(0);
        app.update(Msg::Monitor(Event::Snapshot(gone)));
        assert_eq!(app.selected_server().map(|s| s.id), Some(id(200)));
    }

    #[test]
    fn stop_and_kill_ask_first() {
        let mut app = app();
        assert_eq!(press(&mut app, KeyCode::Char('x')), None);
        assert!(matches!(app.overlay, Some(Overlay::Confirm { .. })));
        assert_eq!(
            press(&mut app, KeyCode::Char('y')),
            send(Request::Stop(id(100)))
        );
        press(&mut app, KeyCode::Char('K'));
        assert_eq!(press(&mut app, KeyCode::Char('n')), None);
        press(&mut app, KeyCode::Char('K'));
        assert_eq!(
            press(&mut app, KeyCode::Enter),
            send(Request::Kill(id(100)))
        );
    }

    #[test]
    fn copy_keys_depend_on_the_pane() {
        let mut app = app();
        logs(&mut app, &["one", "\u{1b}[32mtwo\u{1b}[0m", "three"], true);
        assert_eq!(
            press(&mut app, KeyCode::Char('y')),
            copied("http://localhost:3000", "the URL")
        );
        assert_eq!(
            press(&mut app, KeyCode::Char('Y')),
            copied("npm run dev", "the command")
        );

        press(&mut app, KeyCode::Tab);
        assert_eq!(app.focus, Focus::Logs);
        assert_eq!(
            press(&mut app, KeyCode::Char('y')),
            copied("three", "1 line")
        );
        press(&mut app, KeyCode::Char('v'));
        press(&mut app, KeyCode::Up);
        press(&mut app, KeyCode::Up);
        assert_eq!(
            press(&mut app, KeyCode::Char('y')),
            copied("one\ntwo\nthree", "3 lines")
        );
        assert_eq!(
            press(&mut app, KeyCode::Char('Y')),
            copied("one\ntwo\nthree", "3 lines")
        );
    }

    #[test]
    fn a_server_without_a_log_offers_its_paddock_run_command() {
        let mut app = app();
        press(&mut app, KeyCode::Down);
        app.take_follow();
        press(&mut app, KeyCode::Tab);
        assert_eq!(
            press(&mut app, KeyCode::Char('Y')),
            copied(
                "cd /home/me/shop && paddock run npm run api",
                "the restart command"
            )
        );
    }

    #[test]
    fn scrolled_logs_stay_put_while_lines_arrive() {
        let mut app = app();
        app.log_height.set(5);
        let many: Vec<String> = (0..50).map(|i| format!("line {i}")).collect();
        let many: Vec<&str> = many.iter().map(String::as_str).collect();
        logs(&mut app, &many, true);
        press(&mut app, KeyCode::PageUp);
        assert_eq!(app.scroll, 4);
        logs(&mut app, &["new"], false);
        assert_eq!(app.scroll, 5);
        press(&mut app, KeyCode::Char('G'));
        assert_eq!(app.scroll, 0);
    }

    #[test]
    fn enter_on_a_port_selects_its_server() {
        let mut app = app();
        press(&mut app, KeyCode::BackTab);
        assert_eq!(app.focus, Focus::Ports);
        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.focus, Focus::Servers);
        assert_eq!(app.selected_server().map(|s| s.id), Some(id(200)));
    }

    #[test]
    fn open_uses_the_selected_port() {
        let mut app = app();
        assert_eq!(
            press(&mut app, KeyCode::Char('o')),
            Some(Effect::Open("http://localhost:3000".into()))
        );
    }

    #[test]
    fn quit_does_not_ask_because_nothing_is_stopped() {
        let mut app = app();
        press(&mut app, KeyCode::Char('q'));
        assert!(app.should_quit());
    }

    #[test]
    fn rebinding_a_key_in_settings_saves_the_config() {
        let mut app = app();
        press(&mut app, KeyCode::Char(','));
        press(&mut app, KeyCode::Tab);
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
    fn renders_a_server_with_logs() {
        let mut app = app();
        logs(
            &mut app,
            &["VITE v8 ready in 201 ms", "Error: something broke"],
            true,
        );
        insta::assert_snapshot!(render(&app, 100, 26));
    }

    #[test]
    fn renders_a_server_without_logs() {
        let mut app = app();
        press(&mut app, KeyCode::Down);
        app.take_follow();
        insta::assert_snapshot!(render(&app, 100, 26));
    }

    #[test]
    fn renders_the_empty_state() {
        let mut app = App::new(Config::default(), "test", false);
        app.theme = Theme::plain();
        app.update(Msg::Monitor(Event::Snapshot(Snapshot::default())));
        insta::assert_snapshot!(render(&app, 100, 24));
    }

    #[test]
    fn renders_help() {
        let mut app = app();
        press(&mut app, KeyCode::Char('?'));
        insta::assert_snapshot!(render(&app, 100, 30));
    }

    #[test]
    fn small_terminals_get_a_message_instead_of_a_broken_layout() {
        let backend = render(&app(), 40, 10);
        assert!(format!("{backend}").contains("Terminal too small"));
    }
}
