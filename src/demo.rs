//! A fake process owner for building and showing the TUI before the real
//! supervisor exists. It speaks the same `ipc` protocol: answers every
//! `Request` with believable state changes, streams coloured log lines for
//! running processes (Expo's QR code included), and keeps the ports list in
//! step.
//!
//! Runs nothing and touches nothing outside its own memory. Remove the
//! `paddock` default to it once the daemon can run real projects.

use std::path::PathBuf;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio::time::{self, Instant};

use crate::ipc::protocol::{Event, Request};
use crate::ipc::transport::ServerEnd;
use crate::model::{
    ListeningPort, ProcessId, ProcessInfo, ProcessState, ProjectInfo, ResourceUsage, Snapshot,
    now_ms,
};

const TICK: Duration = Duration::from_millis(100);
const BOOT_TIME: Duration = Duration::from_millis(1400);
const STOP_TIME: Duration = Duration::from_millis(600);

/// Runs until the TUI hangs up.
pub async fn run(mut server: ServerEnd) {
    let mut demo = Demo::new();
    if send_all(&server.events, demo.opening_events())
        .await
        .is_err()
    {
        return;
    }
    let mut tick = time::interval(TICK);
    loop {
        let events = tokio::select! {
            request = server.requests.recv() => match request {
                Some(request) => demo.handle(request, Instant::now()),
                None => return,
            },
            _ = tick.tick() => demo.tick(Instant::now()),
        };
        if send_all(&server.events, events).await.is_err() {
            return;
        }
    }
}

async fn send_all(tx: &mpsc::Sender<Event>, events: Vec<Event>) -> Result<(), ()> {
    for event in events {
        tx.send(event).await.map_err(|_| ())?;
    }
    Ok(())
}

struct DemoProcess {
    info: ProcessInfo,
    /// The pid it gets when it runs.
    next_pid: u32,
    boot: Vec<String>,
    script: &'static [&'static str],
    /// Where the next line of `script` comes from.
    cursor: usize,
    /// Ticks between log lines, so processes talk at different speeds.
    every: u32,
}

struct Demo {
    projects: Vec<String>,
    processes: Vec<DemoProcess>,
    /// A listener Paddock did not start. macOS AirPlay Receiver really does
    /// sit on port 5000.
    foreign: Option<ListeningPort>,
    /// State changes waiting for their moment, e.g. Starting then Running.
    pending: Vec<(Instant, ProcessId, ProcessState)>,
    ticks: u32,
}

struct Spec {
    project: &'static str,
    name: &'static str,
    command: &'static str,
    source: &'static str,
    port: Option<u16>,
    state: ProcessState,
    boot: Vec<String>,
    script: &'static [&'static str],
    every: u32,
}

impl Demo {
    fn new() -> Self {
        let mut demo = Self {
            projects: Vec::new(),
            processes: Vec::new(),
            foreign: Some(ListeningPort {
                port: 5000,
                pid: 812,
                command: "ControlCenter".into(),
                owner: None,
            }),
            pending: Vec::new(),
            ticks: 0,
        };
        let specs = [
            Spec {
                project: "storefront",
                name: "web",
                command: "yarn next dev",
                source: "package.json script \"dev\"",
                port: Some(3000),
                state: ProcessState::Running,
                boot: lines(NEXT_BOOT),
                script: NEXT_LOGS,
                every: 7,
            },
            Spec {
                project: "storefront",
                name: "api",
                command: "yarn tsx watch src/index.ts",
                source: "package.json script \"api\"",
                port: Some(4000),
                state: ProcessState::Running,
                boot: lines(API_BOOT),
                script: API_LOGS,
                every: 5,
            },
            Spec {
                project: "storefront",
                name: "db",
                command: "docker compose up postgres",
                source: "compose.yaml service \"postgres\"",
                port: Some(5432),
                state: ProcessState::Running,
                boot: lines(POSTGRES_BOOT),
                script: POSTGRES_LOGS,
                every: 23,
            },
            Spec {
                project: "mobile-app",
                name: "expo",
                command: "yarn expo start",
                source: "package.json script \"start\"",
                port: Some(8081),
                state: ProcessState::Running,
                boot: expo_boot(),
                script: EXPO_LOGS,
                every: 11,
            },
            Spec {
                project: "docs-site",
                name: "dev",
                command: "pnpm astro dev",
                source: "package.json script \"dev\"",
                port: Some(4321),
                state: ProcessState::Stopped,
                boot: lines(ASTRO_BOOT),
                script: ASTRO_LOGS,
                every: 13,
            },
            Spec {
                project: "billing",
                name: "worker",
                command: "cargo run --bin worker",
                source: "Cargo.toml binary \"worker\"",
                port: None,
                state: ProcessState::Crashed(Some(101)),
                boot: lines(WORKER_BOOT),
                script: WORKER_LOGS,
                every: 9,
            },
        ];
        for spec in specs {
            demo.add(spec);
        }
        demo
    }

    fn add(&mut self, spec: Spec) {
        if !self.projects.iter().any(|p| p == spec.project) {
            self.projects.push(spec.project.to_owned());
        }
        let next_pid = 41000 + 137 * self.processes.len() as u32;
        let running = spec.state == ProcessState::Running;
        self.processes.push(DemoProcess {
            info: ProcessInfo {
                id: ProcessId::new(spec.project, spec.name),
                command: spec.command.to_owned(),
                state: spec.state,
                port: spec.port,
                usage: running.then(|| fake_usage(next_pid, 0)),
                cwd: project_path(spec.project),
                source: spec.source.to_owned(),
                pid: running.then_some(next_pid),
                // Pretend the running ones have been up for a while.
                started_at_ms: running
                    .then(|| now_ms().saturating_sub(60_000 * (3 + next_pid as u64 % 40))),
                restarts: 0,
            },
            next_pid,
            boot: spec.boot,
            script: spec.script,
            cursor: 0,
            every: spec.every,
        });
    }

    fn find(&self, id: &ProcessId) -> Option<&DemoProcess> {
        self.processes.iter().find(|p| &p.info.id == id)
    }

    fn find_mut(&mut self, id: &ProcessId) -> Option<&mut DemoProcess> {
        self.processes.iter_mut().find(|p| &p.info.id == id)
    }

    fn snapshot(&self) -> Snapshot {
        let projects = self
            .projects
            .iter()
            .map(|name| ProjectInfo {
                name: name.clone(),
                path: project_path(name),
                processes: self
                    .processes
                    .iter()
                    .filter(|p| &p.info.id.project == name)
                    .map(|p| p.info.clone())
                    .collect(),
            })
            .collect();
        Snapshot {
            projects,
            ports: self.ports(),
        }
    }

    fn ports(&self) -> Vec<ListeningPort> {
        let mut ports: Vec<ListeningPort> = self
            .processes
            .iter()
            .filter(|p| p.info.state == ProcessState::Running)
            .filter_map(|p| {
                Some(ListeningPort {
                    port: p.info.port?,
                    pid: p.info.pid?,
                    command: p.info.command.clone(),
                    owner: Some(p.info.id.clone()),
                })
            })
            .chain(self.foreign.clone())
            .collect();
        ports.sort_by_key(|p| p.port);
        ports
    }

    /// The snapshot, then some history so the log pane is not empty.
    fn opening_events(&mut self) -> Vec<Event> {
        let mut events = vec![Event::Snapshot(self.snapshot())];
        for process in &mut self.processes {
            let mut lines = process.boot.clone();
            match process.info.state {
                ProcessState::Running => {
                    for _ in 0..12 {
                        lines.push(next_line(process));
                    }
                }
                ProcessState::Crashed(_) => {
                    lines.extend(process.script.iter().map(|l| l.to_string()));
                }
                _ => lines.clear(),
            }
            if !lines.is_empty() {
                events.push(Event::Output {
                    id: process.info.id.clone(),
                    lines,
                });
            }
        }
        events
    }

    fn handle(&mut self, request: Request, now: Instant) -> Vec<Event> {
        match request {
            Request::Start(id) => self.start_stop(id, true, false, now),
            Request::Stop(id) => self.start_stop(id, false, true, now),
            Request::Restart(id) => self.start_stop(id, true, true, now),
            Request::Kill(id) => self.kill(&id),
            Request::SetPort { id, port } => self.set_port(id, port, now),
            Request::Move { id, project } => self.move_to(id, project),
            Request::KillPort { port, pid } => self.kill_port(port, pid),
            Request::AddProject(_) | Request::RemoveProject(_) => vec![Event::Notice(
                "The demo cannot add or remove projects. Run paddock without --demo.".into(),
            )],
        }
    }

    fn start_stop(&mut self, id: ProcessId, start: bool, stop: bool, now: Instant) -> Vec<Event> {
        let Some(process) = self.find(&id) else {
            return Vec::new();
        };
        let up = process.info.state.is_up();
        self.pending.retain(|(_, pending, _)| *pending != id);

        let mut events = Vec::new();
        let mut boot_at = now;
        if stop && up {
            events.extend(self.set_state(&id, ProcessState::Stopping));
            events.push(output(&id, "^C received, shutting down"));
            self.pending
                .push((now + STOP_TIME, id.clone(), ProcessState::Stopped));
            boot_at = now + STOP_TIME;
        }
        if start && (!up || stop) {
            if stop && let Some(process) = self.find_mut(&id) {
                process.info.restarts += 1;
            }
            self.pending
                .push((boot_at, id.clone(), ProcessState::Starting));
            self.pending
                .push((boot_at + BOOT_TIME, id, ProcessState::Running));
        }
        events
    }

    fn kill(&mut self, id: &ProcessId) -> Vec<Event> {
        let Some(process) = self.find(id) else {
            return Vec::new();
        };
        if !process.info.state.is_up() && process.info.state != ProcessState::Stopping {
            return vec![Event::Notice(format!("{id} is not running."))];
        }
        self.pending.retain(|(_, pending, _)| pending != id);
        let mut events = vec![output(id, "\u{1b}[31mKilled: 9\u{1b}[0m")];
        events.extend(self.set_state(id, ProcessState::Crashed(None)));
        events
    }

    fn set_port(&mut self, id: ProcessId, port: u16, now: Instant) -> Vec<Event> {
        let taken_by = self
            .ports()
            .into_iter()
            .find(|p| p.port == port && p.owner.as_ref() != Some(&id));
        if let Some(taken) = taken_by {
            let who = taken
                .owner
                .map_or(format!("{} (pid {})", taken.command, taken.pid), |o| {
                    o.to_string()
                });
            return vec![Event::Notice(format!(
                "Port {port} is already used by {who}."
            ))];
        }
        let Some(process) = self.find_mut(&id) else {
            return Vec::new();
        };
        process.info.port = Some(port);
        let running = process.info.state.is_up();
        let mut events = vec![
            Event::Snapshot(self.snapshot()),
            Event::Notice(format!("{id} now uses port {port}.")),
        ];
        if running {
            events.extend(self.start_stop(id, true, true, now));
        }
        events
    }

    fn move_to(&mut self, id: ProcessId, project: String) -> Vec<Event> {
        let to = ProcessId::new(project.clone(), id.name.clone());
        if self.find(&to).is_some() {
            return vec![Event::Notice(format!(
                "{project} already has a process called {}.",
                id.name
            ))];
        }
        let Some(process) = self.find_mut(&id) else {
            return Vec::new();
        };
        process.info.id = to.clone();
        for (_, pending, _) in &mut self.pending {
            if *pending == id {
                *pending = to.clone();
            }
        }
        if !self.projects.contains(&project) {
            self.projects.push(project.clone());
        }
        let processes = &self.processes;
        self.projects
            .retain(|name| processes.iter().any(|p| &p.info.id.project == name));
        vec![
            Event::Renamed {
                from: id.clone(),
                to,
            },
            Event::Snapshot(self.snapshot()),
            Event::Notice(format!("Moved {} to {project}.", id.name)),
        ]
    }

    fn kill_port(&mut self, port: u16, pid: u32) -> Vec<Event> {
        if let Some(foreign) = self.foreign.take_if(|f| f.port == port && f.pid == pid) {
            return vec![
                Event::Ports(self.ports()),
                Event::Notice(format!(
                    "Killed {} (pid {pid}). Port {port} is free.",
                    foreign.command
                )),
            ];
        }
        let owner = self
            .processes
            .iter()
            .find(|p| p.info.port == Some(port) && p.info.pid == Some(pid))
            .map(|p| p.info.id.clone());
        match owner {
            Some(id) => self.kill(&id),
            None => vec![Event::Notice(format!(
                "Nothing listens on port {port} any more."
            ))],
        }
    }

    fn tick(&mut self, now: Instant) -> Vec<Event> {
        self.ticks += 1;
        let mut events = Vec::new();

        let (due, later): (Vec<_>, Vec<_>) =
            self.pending.drain(..).partition(|(at, _, _)| *at <= now);
        self.pending = later;
        for (_, id, state) in due {
            events.extend(self.set_state(&id, state));
            if state == ProcessState::Starting
                && let Some(process) = self.find_mut(&id)
            {
                process.cursor = 0;
                let lines = process.boot.clone();
                events.push(Event::Output { id, lines });
            }
        }

        let ticks = self.ticks;
        if ticks.is_multiple_of(10) {
            let usage = self
                .processes
                .iter()
                .filter(|p| p.info.state == ProcessState::Running)
                .map(|p| (p.info.id.clone(), fake_usage(p.next_pid, ticks)))
                .collect();
            events.push(Event::Usage(usage));
        }
        for process in &mut self.processes {
            if process.info.state != ProcessState::Running || !ticks.is_multiple_of(process.every) {
                continue;
            }
            let line = next_line(process);
            events.push(Event::Output {
                id: process.info.id.clone(),
                lines: vec![line],
            });
        }
        events
    }

    fn set_state(&mut self, id: &ProcessId, state: ProcessState) -> Vec<Event> {
        let Some(process) = self.find_mut(id) else {
            return Vec::new();
        };
        let had_port = process.info.state == ProcessState::Running && process.info.port.is_some();
        process.info.state = state;
        match state {
            ProcessState::Starting => {
                process.info.pid = Some(process.next_pid);
                process.info.started_at_ms = Some(now_ms());
            }
            ProcessState::Running | ProcessState::Stopping => {}
            _ => {
                process.info.pid = None;
                process.info.started_at_ms = None;
            }
        }
        let has_port = state == ProcessState::Running && process.info.port.is_some();

        let mut events = vec![Event::State {
            id: id.clone(),
            state,
        }];
        if had_port != has_port {
            events.push(Event::Ports(self.ports()));
        }
        if state == ProcessState::Starting {
            // pid and start time changed.
            events.push(Event::Snapshot(self.snapshot()));
        }
        events
    }
}

fn project_path(project: &str) -> PathBuf {
    let home = std::env::var_os("HOME").map_or_else(|| PathBuf::from("~"), PathBuf::from);
    home.join("Projects").join(project)
}

fn output(id: &ProcessId, line: &str) -> Event {
    Event::Output {
        id: id.clone(),
        lines: vec![line.to_owned()],
    }
}

fn lines(text: &[&str]) -> Vec<String> {
    text.iter().map(|l| l.to_string()).collect()
}

fn next_line(process: &mut DemoProcess) -> String {
    let line = process.script[process.cursor % process.script.len()];
    process.cursor += 1;
    line.to_owned()
}

/// Wobbles around a per-process baseline so the numbers look alive.
fn fake_usage(pid: u32, ticks: u32) -> ResourceUsage {
    let base = (pid % 7) as f32 * 1.5 + 0.4;
    let wobble = ((ticks / 3 + pid) % 10) as f32 / 4.0;
    ResourceUsage {
        cpu_percent: base + wobble,
        memory_bytes: (60 + (pid % 11) as u64 * 23) * 1024 * 1024,
    }
}

/// What `expo start` prints: a header, a QR code drawn with half blocks,
/// and the key menu, all with Expo's colours.
fn expo_boot() -> Vec<String> {
    let mut out = vec![
        "\u{1b}[1mStarting project at\u{1b}[22m ~/Projects/mobile-app".to_owned(),
        "\u{1b}[2mStarting Metro Bundler\u{1b}[22m".to_owned(),
    ];
    out.extend(fake_qr());
    out.extend(lines(EXPO_MENU));
    out
}

/// A QR-shaped pattern (finder squares, timing lines, noise). It is not a
/// real code: the demo has nothing to point a phone at.
fn fake_qr() -> Vec<String> {
    const SIZE: usize = 25;
    let mut seed: u32 = 0x5eed_1234;
    let mut dark = [[false; SIZE]; SIZE];
    for row in dark.iter_mut() {
        for cell in row.iter_mut() {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            *cell = (seed >> 16) & 1 == 1;
        }
    }
    for (top, left) in [(0, 0), (0, SIZE - 7), (SIZE - 7, 0)] {
        for r in 0..7 {
            for c in 0..7 {
                let ring = r.min(c).min(6 - r).min(6 - c);
                dark[top + r][left + c] = ring != 1;
            }
        }
    }
    for (i, row) in dark.iter_mut().enumerate().take(SIZE - 8).skip(8) {
        row[6] = i % 2 == 0;
    }
    for (i, cell) in dark[6].iter_mut().enumerate().take(SIZE - 8).skip(8) {
        *cell = i % 2 == 0;
    }

    // One light module of quiet zone around it, then light modules drawn as
    // blocks, the way terminal QR printers do on dark backgrounds.
    let light = |r: isize, c: isize| -> bool {
        if r < 0 || c < 0 || r >= SIZE as isize || c >= SIZE as isize {
            return true;
        }
        !dark[r as usize][c as usize]
    };
    (-1..SIZE as isize + 1)
        .step_by(2)
        .map(|r| {
            (-1..SIZE as isize + 1)
                .map(|c| match (light(r, c), light(r + 1, c)) {
                    (true, true) => '█',
                    (true, false) => '▀',
                    (false, true) => '▄',
                    (false, false) => ' ',
                })
                .collect()
        })
        .collect()
}

const NEXT_BOOT: &[&str] = &[
    "   \u{1b}[1m\u{1b}[35m▲ Next.js 15.3.1\u{1b}[39m\u{1b}[22m",
    "   - Local:        \u{1b}[36mhttp://localhost:3000\u{1b}[39m",
    "",
    " \u{1b}[32m✓\u{1b}[39m Starting...",
    " \u{1b}[32m✓\u{1b}[39m Ready in 1612ms",
];
const NEXT_LOGS: &[&str] = &[
    " \u{1b}[37m○\u{1b}[39m Compiling /products ...",
    " \u{1b}[32m✓\u{1b}[39m Compiled /products in 412ms \u{1b}[2m(1023 modules)\u{1b}[22m",
    " GET /products \u{1b}[32m200\u{1b}[39m in 486ms",
    " GET /api/cart \u{1b}[32m200\u{1b}[39m in 31ms",
    " GET /products/42 \u{1b}[32m200\u{1b}[39m in 92ms",
    " \u{1b}[33m⚠\u{1b}[39m Fast Refresh had to perform a full reload",
    " GET /checkout \u{1b}[31m500\u{1b}[39m in 211ms",
    " \u{1b}[32m✓\u{1b}[39m Compiled in 188ms \u{1b}[2m(998 modules)\u{1b}[22m",
];
const API_BOOT: &[&str] = &[
    "[api] loading .env.development",
    "[api] connected to postgres at localhost:5432",
    "[api] listening on http://localhost:4000",
];
const API_LOGS: &[&str] = &[
    "[api] GET /health 200 2ms",
    "[api] POST /orders 201 18ms",
    "[api] GET /orders/9812 200 6ms",
    "[api] GET /products?page=2 200 11ms",
    "[api] WARN slow query: SELECT * FROM line_items (1203ms)",
    "[api] POST /checkout 200 64ms",
    "[api] Error: connect ECONNREFUSED 127.0.0.1:6379 (redis cache, falling back)",
    "[api] GET /health 200 1ms",
    "[api] PATCH /cart/31 200 9ms",
];
const POSTGRES_BOOT: &[&str] = &[
    "db-1  | PostgreSQL init process complete; ready for start up.",
    "db-1  | LOG:  database system is ready to accept connections",
];
const POSTGRES_LOGS: &[&str] = &[
    "db-1  | LOG:  checkpoint starting: time",
    "db-1  | LOG:  checkpoint complete: wrote 42 buffers (0.3%)",
    "db-1  | LOG:  automatic vacuum of table \"shop.public.carts\"",
];
const EXPO_MENU: &[&str] = &[
    "",
    "› Metro waiting on \u{1b}[4mexp://192.168.1.24:8081\u{1b}[24m",
    "› Scan the QR code above with Expo Go (Android) or the Camera app (iOS)",
    "",
    "› Using \u{1b}[1mExpo Go\u{1b}[22m",
    "› Press \u{1b}[1ms\u{1b}[22m │ switch to development build",
    "",
    "› Press \u{1b}[1ma\u{1b}[22m │ open Android",
    "› Press \u{1b}[1mi\u{1b}[22m │ open iOS simulator",
    "› Press \u{1b}[1mw\u{1b}[22m │ open web",
    "",
    "› Press \u{1b}[1mr\u{1b}[22m │ reload app",
    "› Press \u{1b}[1mm\u{1b}[22m │ toggle menu",
    "",
    "Logs for your project will appear below. \u{1b}[2mPress Ctrl+C to exit.\u{1b}[22m",
];
const EXPO_LOGS: &[&str] = &[
    "\u{1b}[32miOS\u{1b}[39m Bundling ░░░░░░░░ 12%\r\u{1b}[32miOS\u{1b}[39m Bundling ▓▓▓▓░░░░ 54%\r\u{1b}[32miOS\u{1b}[39m Bundled 812ms index.js \u{1b}[2m(1243 modules)\u{1b}[22m",
    " \u{1b}[2mLOG\u{1b}[22m  [cart] item added: sku-1042",
    " \u{1b}[2mLOG\u{1b}[22m  [auth] session refreshed",
    "\u{1b}[32miOS\u{1b}[39m Bundled 97ms index.js \u{1b}[2m(1 module)\u{1b}[22m",
    " \u{1b}[33mWARN\u{1b}[39m  VirtualizedList: You have a large list that is slow to update",
    " \u{1b}[2mLOG\u{1b}[22m  [nav] Home → ProductDetail",
];
const ASTRO_BOOT: &[&str] = &[
    " \u{1b}[42m\u{1b}[30m astro \u{1b}[39m\u{1b}[49m \u{1b}[32mv5.7.0\u{1b}[39m ready in 389 ms",
    "",
    "┃ Local    \u{1b}[36mhttp://localhost:4321/\u{1b}[39m",
    "",
    "\u{1b}[2mwatching for file changes...\u{1b}[22m",
];
const ASTRO_LOGS: &[&str] = &[
    "\u{1b}[2m12:01:03\u{1b}[22m \u{1b}[32m[200]\u{1b}[39m / 4ms",
    "\u{1b}[2m12:01:09\u{1b}[22m \u{1b}[32m[200]\u{1b}[39m /guides/getting-started 11ms",
    "\u{1b}[2m12:01:12\u{1b}[22m \u{1b}[34m[content]\u{1b}[39m Synced content",
    "\u{1b}[2m12:01:20\u{1b}[22m \u{1b}[33m[404]\u{1b}[39m /favicon.svg 2ms",
];
const WORKER_BOOT: &[&str] = &[
    "\u{1b}[1m\u{1b}[32m   Compiling\u{1b}[0m billing v0.4.0 (~/Projects/billing)",
    "\u{1b}[1m\u{1b}[32m    Finished\u{1b}[0m `dev` profile [unoptimized + debuginfo] target(s) in 3.82s",
    "\u{1b}[1m\u{1b}[32m     Running\u{1b}[0m `target/debug/worker`",
];
const WORKER_LOGS: &[&str] = &[
    "\u{1b}[2m2026-10-07T12:01:03Z\u{1b}[0m \u{1b}[32m INFO\u{1b}[0m worker: polling invoices queue",
    "\u{1b}[2m2026-10-07T12:01:03Z\u{1b}[0m \u{1b}[32m INFO\u{1b}[0m worker: picked up 3 jobs",
    "\u{1b}[2m2026-10-07T12:01:04Z\u{1b}[0m \u{1b}[31mERROR\u{1b}[0m worker: failed to render invoice inv_8812: missing tax rate for region \"NO\"",
    "thread 'main' panicked at src/bin/worker.rs:88:14:",
    "called `Option::unwrap()` on a `None` value",
    "note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace",
];

#[cfg(test)]
mod tests {
    use super::*;

    fn id(project: &str, name: &str) -> ProcessId {
        ProcessId::new(project, name)
    }

    fn states(events: &[Event]) -> Vec<(ProcessId, ProcessState)> {
        events
            .iter()
            .filter_map(|e| match e {
                Event::State { id, state } => Some((id.clone(), *state)),
                _ => None,
            })
            .collect()
    }

    fn notices(events: &[Event]) -> Vec<String> {
        events
            .iter()
            .filter_map(|e| match e {
                Event::Notice(text) => Some(text.clone()),
                _ => None,
            })
            .collect()
    }

    fn has_port(events: &[Event], port: u16) -> Option<bool> {
        events.iter().rev().find_map(|e| match e {
            Event::Ports(ports) => Some(ports.iter().any(|p| p.port == port)),
            _ => None,
        })
    }

    #[test]
    fn opening_events_start_with_a_snapshot() {
        let mut demo = Demo::new();
        let events = demo.opening_events();
        let Event::Snapshot(snapshot) = &events[0] else {
            panic!("first event is not a snapshot");
        };
        assert_eq!(snapshot.projects.len(), 4);
        assert!(snapshot.ports.iter().any(|p| p.owner.is_none()));
    }

    #[test]
    fn start_goes_through_starting_to_running_and_opens_the_port() {
        let mut demo = Demo::new();
        let docs = id("docs-site", "dev");
        let t0 = Instant::now();

        assert!(demo.handle(Request::Start(docs.clone()), t0).is_empty());
        assert_eq!(
            states(&demo.tick(t0)),
            vec![(docs.clone(), ProcessState::Starting)]
        );
        let events = demo.tick(t0 + BOOT_TIME);
        assert_eq!(states(&events), vec![(docs, ProcessState::Running)]);
        assert_eq!(has_port(&events, 4321), Some(true));
    }

    #[test]
    fn stop_frees_the_port() {
        let mut demo = Demo::new();
        let web = id("storefront", "web");
        let t0 = Instant::now();

        let events = demo.handle(Request::Stop(web.clone()), t0);
        assert_eq!(states(&events), vec![(web.clone(), ProcessState::Stopping)]);
        assert_eq!(has_port(&events, 3000), Some(false));
        assert_eq!(
            states(&demo.tick(t0 + STOP_TIME)),
            vec![(web, ProcessState::Stopped)]
        );
    }

    #[test]
    fn kill_is_immediate() {
        let mut demo = Demo::new();
        let web = id("storefront", "web");
        let events = demo.handle(Request::Kill(web.clone()), Instant::now());
        assert_eq!(states(&events), vec![(web, ProcessState::Crashed(None))]);
        assert_eq!(has_port(&events, 3000), Some(false));
    }

    #[test]
    fn a_taken_port_is_refused() {
        let mut demo = Demo::new();
        let events = demo.handle(
            Request::SetPort {
                id: id("storefront", "web"),
                port: 5000,
            },
            Instant::now(),
        );
        assert_eq!(
            notices(&events),
            vec!["Port 5000 is already used by ControlCenter (pid 812).".to_string()]
        );
    }

    #[test]
    fn a_new_port_restarts_a_running_process() {
        let mut demo = Demo::new();
        let web = id("storefront", "web");
        let t0 = Instant::now();
        let events = demo.handle(
            Request::SetPort {
                id: web.clone(),
                port: 3100,
            },
            t0,
        );
        assert_eq!(states(&events), vec![(web.clone(), ProcessState::Stopping)]);
        demo.tick(t0 + STOP_TIME);
        let events = demo.tick(t0 + STOP_TIME + BOOT_TIME);
        assert_eq!(has_port(&events, 3100), Some(true));
        assert_eq!(demo.find(&web).map(|p| p.info.restarts), Some(1));
    }

    #[test]
    fn moving_renames_and_drops_empty_projects() {
        let mut demo = Demo::new();
        let from = id("billing", "worker");
        let events = demo.handle(
            Request::Move {
                id: from.clone(),
                project: "storefront".into(),
            },
            Instant::now(),
        );
        assert!(events.contains(&Event::Renamed {
            from,
            to: id("storefront", "worker")
        }));
        assert!(!demo.projects.contains(&"billing".to_string()));
    }

    #[test]
    fn killing_the_foreign_listener_frees_its_port() {
        let mut demo = Demo::new();
        let events = demo.handle(
            Request::KillPort {
                port: 5000,
                pid: 812,
            },
            Instant::now(),
        );
        assert_eq!(has_port(&events, 5000), Some(false));
    }

    #[test]
    fn the_fake_qr_code_is_square() {
        let qr = fake_qr();
        assert_eq!(qr.len(), 14);
        assert!(qr.iter().all(|row| row.chars().count() == 27));
    }
}
