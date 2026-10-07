//! TUI state and the update function.
//!
//! `App::update` is the only place state changes. It takes a `Msg` (a key,
//! a timer tick, a monitor event, the result of a side effect) and may
//! return an `Effect` for the event loop to carry out: send a request, copy
//! text, open a URL, save the config. It never touches the terminal or the
//! disk, so it is tested directly. Popups handle their own keys in
//! `overlay.rs` and `settings.rs`.

use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::widgets::{Block, Paragraph, Widget};

use super::keys::{Action, Keymap};
use super::overlay::Overlay;
use super::settings::Settings;
use super::theme::Theme;
use super::{details, help, overlay, settings, sidebar, splash, status_bar};
use crate::config::Config;
use crate::ipc::protocol::{Event, Request};
use crate::model::{ListeningPort, Server, ServerId, ServerState, Snapshot};

const MIN_WIDTH: u16 = 70;
const MIN_HEIGHT: u16 = 14;
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
                self.focus = Focus::Servers;
                return None;
            }
            KeyCode::Enter if self.focus == Focus::Ports => {
                if let Some(owner) = self.selected_listener().map(|l| l.owner) {
                    self.selected = Some(owner);
                    self.focus = Focus::Servers;
                }
                return None;
            }
            _ => {}
        }
        self.on_action(self.keymap.action(key)?)
    }

    fn on_action(&mut self, action: Action) -> Option<Effect> {
        match action {
            Action::Up => self.move_by(-1),
            Action::Down => self.move_by(1),
            Action::NextPane | Action::PrevPane => {
                self.focus = match self.focus {
                    Focus::Servers => Focus::Ports,
                    Focus::Ports => Focus::Servers,
                };
            }
            Action::Open => return self.open(),
            Action::Copy => return self.copy_url(),
            Action::CopyAll => {
                let server = self.selected_server()?;
                return Some(Effect::Copy {
                    text: server.command.clone(),
                    what: "the command".into(),
                });
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
            Focus::Ports => {
                let last = self.snapshot.ports.len().saturating_sub(1);
                self.selected_port = self.selected_port.saturating_add_signed(step).min(last);
            }
        }
    }

    /// The port the user is pointing at: the selected row in the Ports pane,
    /// else the selected server's first port.
    fn target_port(&self) -> Option<u16> {
        match self.focus {
            Focus::Ports => self.selected_listener().map(|l| l.port),
            Focus::Servers => self.selected_server()?.ports.first().copied(),
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

    fn confirm_stop(&mut self, force: bool) {
        let target = match self.focus {
            Focus::Ports => self.selected_listener().map(|l| l.owner),
            Focus::Servers => self.selected_server().map(|s| s.id),
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
            Event::Notice(text) => self.notify(NoticeKind::Info, text),
        }
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
        details::render(self, main, buf);
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

    fn server(pid: u32, name: &str, ports: &[u16]) -> Server {
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
        }
    }

    fn snapshot() -> Snapshot {
        let web = server(100, "dev", &[3000]);
        Snapshot {
            projects: vec![
                Project {
                    name: "shop".into(),
                    path: PathBuf::from("/home/me/shop"),
                    kind: "node".into(),
                    servers: vec![web.clone(), server(200, "api", &[4000])],
                },
                Project {
                    name: "docs".into(),
                    path: PathBuf::from("/home/me/docs"),
                    kind: "folder".into(),
                    servers: vec![server(300, "http.server", &[8000])],
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
        app
    }

    pub(crate) fn press(app: &mut App, code: KeyCode) -> Option<Effect> {
        app.update(Msg::Key(KeyEvent::new(code, KeyModifiers::NONE)))
    }

    fn send(request: Request) -> Option<Effect> {
        Some(Effect::Send(request))
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
    fn open_and_copy_use_the_selected_server() {
        let mut app = app();
        assert_eq!(
            press(&mut app, KeyCode::Char('o')),
            Some(Effect::Open("http://localhost:3000".into()))
        );
        assert_eq!(
            press(&mut app, KeyCode::Char('y')),
            copied("http://localhost:3000", "the URL")
        );
        assert_eq!(
            press(&mut app, KeyCode::Char('Y')),
            copied("npm run dev", "the command")
        );
    }

    #[test]
    fn the_ports_pane_targets_its_row() {
        let mut app = app();
        press(&mut app, KeyCode::Tab);
        assert_eq!(app.focus, Focus::Ports);
        press(&mut app, KeyCode::Down);
        assert_eq!(
            press(&mut app, KeyCode::Char('o')),
            Some(Effect::Open("http://localhost:4000".into()))
        );
        press(&mut app, KeyCode::Char('x'));
        assert_eq!(
            press(&mut app, KeyCode::Char('y')),
            send(Request::Stop(id(200)))
        );
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.focus, Focus::Servers);
        assert_eq!(app.selected_server().map(|s| s.id), Some(id(200)));
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
    fn renders_the_dashboard() {
        insta::assert_snapshot!(render(&app(), 100, 24));
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
