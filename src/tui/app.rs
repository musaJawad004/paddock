//! TUI state and the update function.
//!
//! `App::update` is the only place state changes: it takes a `Msg` (a key
//! press or a backend event) and may return a `Request` for the backend. It
//! never touches the terminal, so it is tested directly. Drawing lives in
//! the view modules; `impl Widget for &App` only lays them out.

use std::collections::{HashMap, VecDeque};

use crossterm::event::KeyEvent;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::widgets::{Paragraph, Widget};

use super::keys::{self, Action};
use super::theme::Theme;
use super::{help, logs_view, sidebar, status_bar};
use crate::ipc::protocol::{Event, Request};
use crate::model::{ProcessId, ProcessInfo, ProcessState, Snapshot};

/// Lines kept per process on the TUI side. The backend keeps the full
/// scrollback; this is only what can be scrolled to without asking for more.
pub const LOG_LIMIT: usize = 5_000;
const PAGE: usize = 10;
const MIN_WIDTH: u16 = 70;
const MIN_HEIGHT: u16 = 14;

pub enum Msg {
    Key(KeyEvent),
    Resize,
    Backend(Event),
    BackendGone,
}

pub struct App {
    pub(super) theme: Theme,
    /// Where the data comes from, shown in the header ("demo data").
    pub(super) source: String,
    pub(super) snapshot: Snapshot,
    pub(super) logs: HashMap<ProcessId, VecDeque<String>>,
    /// Index into `processes()`, the flattened list across projects.
    pub(super) selected: usize,
    /// Lines scrolled up from the newest. 0 follows new output.
    pub(super) scroll: usize,
    pub(super) show_help: bool,
    /// One-line message in the status bar until the next key press.
    pub(super) notice: Option<String>,
    dirty: bool,
    quit: bool,
}

impl App {
    pub fn new(theme: Theme, source: impl Into<String>) -> Self {
        Self {
            theme,
            source: source.into(),
            snapshot: Snapshot::default(),
            logs: HashMap::new(),
            selected: 0,
            scroll: 0,
            show_help: false,
            notice: None,
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

    pub(super) fn processes(&self) -> impl Iterator<Item = &ProcessInfo> {
        self.snapshot.projects.iter().flat_map(|p| &p.processes)
    }

    pub(super) fn selected_process(&self) -> Option<&ProcessInfo> {
        self.processes().nth(self.selected)
    }

    pub(super) fn selected_logs(&self) -> Option<&VecDeque<String>> {
        self.logs.get(&self.selected_process()?.id)
    }

    pub fn update(&mut self, msg: Msg) -> Option<Request> {
        self.dirty = true;
        match msg {
            Msg::Key(key) => {
                self.notice = None;
                self.on_action(keys::action_for(key)?)
            }
            Msg::Resize => None,
            Msg::Backend(event) => {
                self.on_event(event);
                None
            }
            Msg::BackendGone => {
                self.notice = Some("The backend stopped. Press q to quit.".into());
                None
            }
        }
    }

    fn on_action(&mut self, action: Action) -> Option<Request> {
        if self.show_help && matches!(action, Action::Help | Action::Quit) {
            self.show_help = false;
            return None;
        }
        match action {
            Action::Up => self.select(self.selected.saturating_sub(1)),
            Action::Down => self.select(self.selected + 1),
            Action::Start => return self.request_start(),
            Action::Stop => return self.request_stop(),
            Action::Restart => {
                return self
                    .selected_process()
                    .map(|p| Request::Restart(p.id.clone()));
            }
            Action::ScrollUp => {
                let len = self.selected_logs().map_or(0, VecDeque::len);
                self.scroll = (self.scroll + PAGE).min(len);
            }
            Action::ScrollDown => self.scroll = self.scroll.saturating_sub(PAGE),
            Action::Follow => self.scroll = 0,
            Action::Help => self.show_help = true,
            Action::Quit => self.quit = true,
        }
        None
    }

    fn select(&mut self, index: usize) {
        let last = self.processes().count().saturating_sub(1);
        let index = index.min(last);
        if index != self.selected {
            self.selected = index;
            self.scroll = 0;
        }
    }

    fn request_start(&mut self) -> Option<Request> {
        let process = self.selected_process()?;
        if process.state.is_up() || process.state == ProcessState::Stopping {
            let notice = format!("{} is already {}.", process.id, process.state.label());
            self.notice = Some(notice);
            return None;
        }
        Some(Request::Start(process.id.clone()))
    }

    fn request_stop(&mut self) -> Option<Request> {
        let process = self.selected_process()?;
        if !process.state.is_up() {
            let notice = format!("{} is not running.", process.id);
            self.notice = Some(notice);
            return None;
        }
        Some(Request::Stop(process.id.clone()))
    }

    fn on_event(&mut self, event: Event) {
        match event {
            Event::Snapshot(snapshot) => {
                self.snapshot = snapshot;
                self.select(self.selected);
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
            Event::Ports(ports) => self.snapshot.ports = ports,
            Event::Usage(usage) => {
                for (id, usage) in usage {
                    if let Some(process) = self.process_mut(&id) {
                        process.usage = Some(usage);
                    }
                }
            }
        }
    }

    fn append_output(&mut self, id: ProcessId, lines: Vec<String>) {
        let added = lines.len();
        let is_selected = self.selected_process().is_some_and(|p| p.id == id);
        let buffer = self.logs.entry(id).or_default();
        buffer.extend(lines);
        let overflow = buffer.len().saturating_sub(LOG_LIMIT);
        buffer.drain(..overflow);
        // Keep a scrolled-up view still while new lines arrive below it.
        if is_selected && self.scroll > 0 {
            self.scroll = (self.scroll + added).min(buffer.len());
        }
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
        logs_view::render(self, main, buf);
        status_bar::render_footer(self, footer, buf);
        if self.show_help {
            help::render(self, area, buf);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crossterm::event::{KeyCode, KeyModifiers};
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::model::ProjectInfo;

    fn process(project: &str, name: &str, state: ProcessState) -> ProcessInfo {
        ProcessInfo {
            id: ProcessId::new(project, name),
            command: format!("run {name}"),
            state,
            port: None,
            usage: None,
        }
    }

    fn app() -> App {
        let mut app = App::new(Theme::plain(), "test");
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
            ports: Vec::new(),
        };
        app.update(Msg::Backend(Event::Snapshot(snapshot)));
        app
    }

    fn press(app: &mut App, code: KeyCode) -> Option<Request> {
        app.update(Msg::Key(KeyEvent::new(code, KeyModifiers::NONE)))
    }

    fn output(app: &mut App, project: &str, name: &str, count: usize) {
        let lines = (0..count).map(|i| format!("line {i}")).collect();
        app.update(Msg::Backend(Event::Output {
            id: ProcessId::new(project, name),
            lines,
        }));
    }

    #[test]
    fn selection_moves_across_projects_and_stops_at_the_ends() {
        let mut app = app();
        press(&mut app, KeyCode::Up);
        assert_eq!(app.selected, 0);
        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Char('j'));
        press(&mut app, KeyCode::Down);
        assert_eq!(
            app.selected_process().map(|p| p.id.to_string()),
            Some("docs/dev".into())
        );
    }

    #[test]
    fn start_is_sent_only_for_processes_that_are_down() {
        let mut app = app();
        assert_eq!(press(&mut app, KeyCode::Char('s')), None);
        assert_eq!(app.notice.as_deref(), Some("shop/web is already running."));

        press(&mut app, KeyCode::Down);
        assert_eq!(
            press(&mut app, KeyCode::Char('s')),
            Some(Request::Start(ProcessId::new("shop", "api")))
        );
    }

    #[test]
    fn stop_is_sent_only_for_processes_that_are_up() {
        let mut app = app();
        assert_eq!(
            press(&mut app, KeyCode::Char('x')),
            Some(Request::Stop(ProcessId::new("shop", "web")))
        );
        press(&mut app, KeyCode::Down);
        assert_eq!(press(&mut app, KeyCode::Char('x')), None);
    }

    #[test]
    fn state_events_update_the_process() {
        let mut app = app();
        let api = ProcessId::new("shop", "api");
        app.update(Msg::Backend(Event::State {
            id: api,
            state: ProcessState::Starting,
        }));
        press(&mut app, KeyCode::Down);
        assert_eq!(
            app.selected_process().map(|p| p.state),
            Some(ProcessState::Starting)
        );
    }

    #[test]
    fn logs_are_capped() {
        let mut app = app();
        output(&mut app, "shop", "web", LOG_LIMIT + 50);
        let logs = app.selected_logs().map(VecDeque::len);
        assert_eq!(logs, Some(LOG_LIMIT));
        assert_eq!(
            app.selected_logs()
                .and_then(|l| l.front())
                .map(String::as_str),
            Some("line 50")
        );
    }

    #[test]
    fn scrolled_view_stays_put_when_output_arrives() {
        let mut app = app();
        output(&mut app, "shop", "web", 100);
        press(&mut app, KeyCode::PageUp);
        assert_eq!(app.scroll, PAGE);
        output(&mut app, "shop", "web", 3);
        assert_eq!(app.scroll, PAGE + 3);
        press(&mut app, KeyCode::End);
        assert_eq!(app.scroll, 0);
    }

    #[test]
    fn quit_closes_help_before_quitting() {
        let mut app = app();
        press(&mut app, KeyCode::Char('?'));
        press(&mut app, KeyCode::Esc);
        assert!(!app.show_help);
        assert!(!app.should_quit());
        press(&mut app, KeyCode::Char('q'));
        assert!(app.should_quit());
    }

    #[test]
    fn ctrl_c_quits() {
        let mut app = app();
        app.update(Msg::Key(KeyEvent::new(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL,
        )));
        assert!(app.should_quit());
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
        app.update(Msg::Backend(Event::Ports(vec![
            crate::model::ListeningPort {
                port: 5000,
                pid: 812,
                command: "ControlCenter".into(),
                owner: None,
            },
        ])));
        insta::assert_snapshot!(render(&app, 100, 22));
    }

    #[test]
    fn renders_help_over_the_dashboard() {
        let mut app = app();
        press(&mut app, KeyCode::Char('?'));
        insta::assert_snapshot!(render(&app, 100, 24));
    }

    #[test]
    fn small_terminals_get_a_message_instead_of_a_broken_layout() {
        let backend = render(&app(), 40, 10);
        let text = format!("{backend}");
        assert!(text.contains("Terminal too small"));
    }
}
