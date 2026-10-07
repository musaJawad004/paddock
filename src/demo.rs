//! A fake process owner for building and showing the TUI before the real
//! supervisor exists. It speaks the same `ipc` protocol: answers Start, Stop
//! and Restart with believable state changes, streams log lines for running
//! processes, and keeps the ports list in step.
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
};

const TICK: Duration = Duration::from_millis(100);
const BOOT_TIME: Duration = Duration::from_millis(1400);
const STOP_TIME: Duration = Duration::from_millis(600);
/// A port nobody in the demo owns: macOS AirPlay Receiver really does sit on 5000.
const FOREIGN_PORT: u16 = 5000;

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
    pid: u32,
    script: &'static [&'static str],
    boot: &'static [&'static str],
    /// Where the next line of `script` comes from.
    cursor: usize,
    /// Ticks until the next log line, so processes talk at different speeds.
    every: u32,
}

struct Demo {
    projects: Vec<(String, PathBuf)>,
    processes: Vec<DemoProcess>,
    /// State changes waiting for their moment, e.g. Starting then Running.
    pending: Vec<(Instant, ProcessId, ProcessState)>,
    ticks: u32,
}

impl Demo {
    fn new() -> Self {
        let mut demo = Self {
            projects: Vec::new(),
            processes: Vec::new(),
            pending: Vec::new(),
            ticks: 0,
        };
        demo.add(
            "storefront",
            "web",
            "yarn next dev",
            Some(3000),
            ProcessState::Running,
            (NEXT_BOOT, NEXT_LOGS, 7),
        );
        demo.add(
            "storefront",
            "api",
            "yarn tsx watch src/index.ts",
            Some(4000),
            ProcessState::Running,
            (API_BOOT, API_LOGS, 5),
        );
        demo.add(
            "storefront",
            "db",
            "docker compose up postgres",
            Some(5432),
            ProcessState::Running,
            (POSTGRES_BOOT, POSTGRES_LOGS, 23),
        );
        demo.add(
            "mobile-app",
            "expo",
            "yarn expo start",
            Some(8081),
            ProcessState::Running,
            (EXPO_BOOT, EXPO_LOGS, 11),
        );
        demo.add(
            "docs-site",
            "dev",
            "pnpm astro dev",
            Some(4321),
            ProcessState::Stopped,
            (ASTRO_BOOT, ASTRO_LOGS, 13),
        );
        demo.add(
            "billing",
            "worker",
            "cargo run --bin worker",
            None,
            ProcessState::Crashed(Some(101)),
            (WORKER_BOOT, WORKER_LOGS, 9),
        );
        demo
    }

    fn add(
        &mut self,
        project: &str,
        name: &str,
        command: &str,
        port: Option<u16>,
        state: ProcessState,
        (boot, script, every): (&'static [&'static str], &'static [&'static str], u32),
    ) {
        if !self.projects.iter().any(|(p, _)| p == project) {
            self.projects.push((
                project.to_owned(),
                PathBuf::from(format!("~/Projects/{project}")),
            ));
        }
        let pid = 41000 + 137 * self.processes.len() as u32;
        self.processes.push(DemoProcess {
            info: ProcessInfo {
                id: ProcessId::new(project, name),
                command: command.to_owned(),
                state,
                port,
                usage: (state == ProcessState::Running).then(|| fake_usage(pid, 0)),
            },
            pid,
            script,
            boot,
            cursor: 0,
            every,
        });
    }

    fn snapshot(&self) -> Snapshot {
        let projects = self
            .projects
            .iter()
            .map(|(name, path)| ProjectInfo {
                name: name.clone(),
                path: path.clone(),
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
                    pid: p.pid,
                    command: p.info.command.clone(),
                    owner: Some(p.info.id.clone()),
                })
            })
            .collect();
        ports.push(ListeningPort {
            port: FOREIGN_PORT,
            pid: 812,
            command: "ControlCenter".into(),
            owner: None,
        });
        ports.sort_by_key(|p| p.port);
        ports
    }

    /// The snapshot, then some history so the log pane is not empty.
    fn opening_events(&mut self) -> Vec<Event> {
        let mut events = vec![Event::Snapshot(self.snapshot())];
        for process in &mut self.processes {
            let mut lines: Vec<String> = process.boot.iter().map(|l| l.to_string()).collect();
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
        let (id, start, stop) = match request {
            Request::Start(id) => (id, true, false),
            Request::Stop(id) => (id, false, true),
            Request::Restart(id) => (id, true, true),
        };
        let Some(process) = self.processes.iter().find(|p| p.info.id == id) else {
            return Vec::new();
        };
        let up = process.info.state.is_up();
        self.pending.retain(|(_, pending, _)| *pending != id);

        let mut events = Vec::new();
        let mut boot_at = now;
        if stop && up {
            events.extend(self.set_state(&id, ProcessState::Stopping));
            events.push(self.output(&id, "^C received, shutting down"));
            self.pending
                .push((now + STOP_TIME, id.clone(), ProcessState::Stopped));
            boot_at = now + STOP_TIME;
        }
        if start && (!up || stop) {
            self.pending
                .push((boot_at, id.clone(), ProcessState::Starting));
            self.pending
                .push((boot_at + BOOT_TIME, id, ProcessState::Running));
        }
        events
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
                && let Some(process) = self.processes.iter_mut().find(|p| p.info.id == id)
            {
                process.cursor = 0;
                let lines = process.boot.iter().map(|l| l.to_string()).collect();
                events.push(Event::Output { id, lines });
            }
        }

        let ticks = self.ticks;
        if ticks.is_multiple_of(10) {
            let usage = self
                .processes
                .iter()
                .filter(|p| p.info.state == ProcessState::Running)
                .map(|p| (p.info.id.clone(), fake_usage(p.pid, ticks)))
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
        let Some(process) = self.processes.iter_mut().find(|p| &p.info.id == id) else {
            return Vec::new();
        };
        let had_port = process.info.state == ProcessState::Running && process.info.port.is_some();
        process.info.state = state;
        let has_port = state == ProcessState::Running && process.info.port.is_some();

        let mut events = vec![Event::State {
            id: id.clone(),
            state,
        }];
        if had_port != has_port {
            events.push(Event::Ports(self.ports()));
        }
        events
    }

    fn output(&self, id: &ProcessId, line: &str) -> Event {
        Event::Output {
            id: id.clone(),
            lines: vec![line.to_owned()],
        }
    }
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

const NEXT_BOOT: &[&str] = &[
    "   ▲ Next.js 15.3.1",
    "   - Local:        http://localhost:3000",
    "",
    " ✓ Starting...",
    " ✓ Ready in 1612ms",
];
const NEXT_LOGS: &[&str] = &[
    " ○ Compiling /products ...",
    " ✓ Compiled /products in 412ms (1023 modules)",
    " GET /products 200 in 486ms",
    " GET /api/cart 200 in 31ms",
    " GET /products/42 200 in 92ms",
    " ⚠ Fast Refresh had to perform a full reload",
    " ✓ Compiled in 188ms (998 modules)",
    " GET / 200 in 54ms",
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
const EXPO_BOOT: &[&str] = &[
    "Starting project at ~/Projects/mobile-app",
    "Starting Metro Bundler",
    "› Metro waiting on exp://192.168.1.24:8081",
    "› Scan the QR code above with Expo Go (Android) or the Camera app (iOS)",
    "› Press i │ open iOS simulator",
    "› Press r │ reload app",
];
const EXPO_LOGS: &[&str] = &[
    "iOS Bundled 812ms index.js (1243 modules)",
    " LOG  [cart] item added: sku-1042",
    " LOG  [auth] session refreshed",
    "iOS Bundled 97ms index.js (1 module)",
    " WARN  VirtualizedList: You have a large list that is slow to update",
    " LOG  [nav] Home → ProductDetail",
];
const ASTRO_BOOT: &[&str] = &[
    " astro  v5.7.0 ready in 389 ms",
    "┃ Local    http://localhost:4321/",
    "watching for file changes...",
];
const ASTRO_LOGS: &[&str] = &[
    "[200] / 4ms",
    "[200] /guides/getting-started 11ms",
    "[content] Synced content",
    "[200] /reference/config 7ms",
];
const WORKER_BOOT: &[&str] = &[
    "   Compiling billing v0.4.0 (~/Projects/billing)",
    "    Finished `dev` profile [unoptimized + debuginfo] target(s) in 3.82s",
    "     Running `target/debug/worker`",
];
const WORKER_LOGS: &[&str] = &[
    "INFO worker: polling invoices queue",
    "INFO worker: picked up 3 jobs",
    "ERROR worker: failed to render invoice inv_8812: missing tax rate for region \"NO\"",
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
        assert!(events.iter().any(|e| matches!(e, Event::Ports(ports)
            if ports.iter().any(|p| p.port == 4321))));
    }

    #[test]
    fn stop_frees_the_port() {
        let mut demo = Demo::new();
        let web = id("storefront", "web");
        let t0 = Instant::now();

        let events = demo.handle(Request::Stop(web.clone()), t0);
        assert_eq!(states(&events), vec![(web.clone(), ProcessState::Stopping)]);
        assert!(events.iter().any(|e| matches!(e, Event::Ports(ports)
            if !ports.iter().any(|p| p.port == 3000))));
        assert_eq!(
            states(&demo.tick(t0 + STOP_TIME)),
            vec![(web, ProcessState::Stopped)]
        );
    }

    #[test]
    fn start_on_a_running_process_does_nothing() {
        let mut demo = Demo::new();
        let t0 = Instant::now();
        assert!(
            demo.handle(Request::Start(id("storefront", "web")), t0)
                .is_empty()
        );
        assert!(states(&demo.tick(t0 + BOOT_TIME)).is_empty());
    }
}
