//! Made-up servers for `paddock --demo`: the same `ipc` protocol as the
//! monitor, with nothing real behind it. Stop and kill change the fake
//! data; nothing on the machine is touched.

use std::path::PathBuf;
use std::time::Duration;

use tokio::time::{self, Instant};

use crate::ipc::protocol::{Event, Request};
use crate::ipc::transport::ServerEnd;
use crate::model::{
    ListeningPort, Project, ResourceUsage, Server, ServerId, ServerState, Snapshot, now_ms,
};

const TICK: Duration = Duration::from_millis(500);
const STOP_TIME: Duration = Duration::from_millis(1200);

/// Runs until the TUI hangs up.
pub async fn run(mut server: ServerEnd) {
    let mut demo = Demo::new();
    let mut tick = time::interval(TICK);
    loop {
        let events = tokio::select! {
            request = server.requests.recv() => match request {
                Some(request) => demo.handle(request),
                None => return,
            },
            _ = tick.tick() => demo.tick(Instant::now()),
        };
        for event in events {
            if server.events.send(event).await.is_err() {
                return;
            }
        }
    }
}

struct Demo {
    projects: Vec<Project>,
    stopping: Vec<(ServerId, Instant)>,
    ticks: u32,
}

fn home() -> PathBuf {
    std::env::var_os("HOME").map_or_else(|| PathBuf::from("~"), PathBuf::from)
}

fn server(
    pid: u32,
    name: &str,
    command: &str,
    folder: &str,
    ports: &[u16],
    processes: usize,
) -> Server {
    let started = now_ms() / 1000 - 60 * (3 + u64::from(pid % 40));
    Server {
        id: ServerId { pid, started },
        name: name.into(),
        command: command.into(),
        cwd: home().join("Projects").join(folder),
        ports: ports.to_vec(),
        processes,
        usage: usage(pid, 0),
        state: ServerState::Running,
    }
}

fn project(name: &str, kind: &str, servers: Vec<Server>) -> Project {
    Project {
        name: name.into(),
        path: home().join("Projects").join(name),
        kind: kind.into(),
        servers,
    }
}

/// Wobbles around a per-server baseline so the numbers look alive.
fn usage(pid: u32, ticks: u32) -> ResourceUsage {
    let base = (pid % 7) as f32 * 1.5 + 0.4;
    let wobble = ((ticks + pid) % 10) as f32 / 4.0;
    ResourceUsage {
        cpu_percent: base + wobble,
        memory_bytes: (60 + u64::from(pid % 11) * 23) * 1024 * 1024,
    }
}

impl Demo {
    fn new() -> Self {
        Self {
            projects: vec![
                project(
                    "billing",
                    "rust",
                    vec![server(
                        52210,
                        "worker",
                        "cargo run --bin worker",
                        "billing",
                        &[8080],
                        2,
                    )],
                ),
                project(
                    "mobile-app",
                    "node",
                    vec![server(
                        48120,
                        "start",
                        "npx expo start",
                        "mobile-app",
                        &[8081],
                        6,
                    )],
                ),
                project(
                    "storefront",
                    "node",
                    vec![
                        server(41000, "dev", "yarn next dev", "storefront", &[3000], 5),
                        server(
                            41137,
                            "api",
                            "yarn tsx watch src/index.ts",
                            "storefront",
                            &[4000],
                            3,
                        ),
                    ],
                ),
            ],
            stopping: Vec::new(),
            ticks: 0,
        }
    }

    fn server(&self, id: ServerId) -> Option<&Server> {
        self.projects
            .iter()
            .flat_map(|p| &p.servers)
            .find(|s| s.id == id)
    }

    fn servers_mut(&mut self) -> impl Iterator<Item = &mut Server> {
        self.projects.iter_mut().flat_map(|p| &mut p.servers)
    }

    fn snapshot(&self) -> Snapshot {
        let mut ports: Vec<ListeningPort> = self
            .projects
            .iter()
            .flat_map(|p| &p.servers)
            .flat_map(|s| {
                s.ports.iter().map(|port| ListeningPort {
                    port: *port,
                    pid: s.id.pid,
                    command: "node".into(),
                    owner: s.id,
                })
            })
            .collect();
        ports.sort_by_key(|p| p.port);
        Snapshot {
            projects: self.projects.clone(),
            ports,
        }
    }

    fn handle(&mut self, request: Request) -> Vec<Event> {
        let notice = match request {
            Request::Stop(id) => {
                let Some(server) = self.servers_mut().find(|s| s.id == id) else {
                    return vec![Event::Notice("It has already stopped.".into())];
                };
                server.state = ServerState::Stopping;
                self.stopping.push((id, Instant::now() + STOP_TIME));
                "Asked the server to stop. It gets 5 seconds before it is killed.".to_owned()
            }
            Request::Kill(id) => {
                let processes = self.server(id).map_or(0, |s| s.processes);
                self.remove(id);
                format!("Killed {processes} processes.")
            }
        };
        vec![Event::Snapshot(self.snapshot()), Event::Notice(notice)]
    }

    fn remove(&mut self, id: ServerId) {
        for project in &mut self.projects {
            project.servers.retain(|s| s.id != id);
        }
        self.projects.retain(|p| !p.servers.is_empty());
        self.stopping.retain(|(s, _)| *s != id);
    }

    fn tick(&mut self, now: Instant) -> Vec<Event> {
        self.ticks += 1;
        let ticks = self.ticks;
        for server in self.servers_mut() {
            server.usage = usage(server.id.pid, ticks);
        }
        let due: Vec<ServerId> = self
            .stopping
            .iter()
            .filter(|(_, at)| now >= *at)
            .map(|(id, _)| *id)
            .collect();
        for id in due {
            self.remove(id);
        }
        vec![Event::Snapshot(self.snapshot())]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn first(demo: &Demo) -> ServerId {
        demo.projects[0].servers[0].id
    }

    #[test]
    fn stop_marks_then_removes() {
        let mut demo = Demo::new();
        let id = first(&demo);
        demo.handle(Request::Stop(id));
        assert_eq!(demo.projects[0].servers[0].state, ServerState::Stopping);
        demo.tick(Instant::now() + STOP_TIME);
        assert!(demo.snapshot().servers().all(|s| s.id != id));
    }

    #[test]
    fn kill_removes_at_once_and_drops_empty_projects() {
        let mut demo = Demo::new();
        let id = first(&demo);
        let before = demo.projects.len();
        demo.handle(Request::Kill(id));
        assert_eq!(demo.projects.len(), before - 1);
    }
}
