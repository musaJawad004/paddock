//! Runs real child processes through the supervisor: tiny shell commands in
//! temporary project folders, never real dev servers. Every test stops what
//! it starts; the supervisor's Drop kills anything left.
//!
//! Socket use is allowed here (and only here): see testing-and-verification.

// Test helpers may panic: a failed setup should fail the test loudly.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::Path;
use std::thread::sleep;
use std::time::{Duration, Instant};

use paddock::daemon::pty::Tagged;
use paddock::daemon::stats::Stats;
use paddock::daemon::supervisor::Supervisor;
use paddock::daemon::{ports, pty};
use paddock::ipc::protocol::{Event, Request};
use paddock::model::{ProcessId, ProcessState};
use tokio::sync::mpsc;

struct Harness {
    supervisor: Supervisor,
    reports: mpsc::Receiver<Tagged>,
    events: Vec<Event>,
    _dir: tempfile::TempDir,
    project: std::path::PathBuf,
}

/// A project folder whose Procfile is `procfile`, already added.
fn harness(procfile: &str) -> Harness {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("proj");
    fs::create_dir(&project).unwrap();
    fs::write(project.join("Procfile"), procfile).unwrap();
    let (tx, reports) = mpsc::channel(1024);
    let mut supervisor = Supervisor::new(dir.path().join("config.toml"), tx);
    let events = supervisor.handle(Request::AddProject(project.clone()), Instant::now());
    Harness {
        supervisor,
        reports,
        events,
        _dir: dir,
        project,
    }
}

impl Harness {
    fn id(&self, name: &str) -> ProcessId {
        ProcessId::new("proj", name)
    }

    fn send(&mut self, request: Request) {
        let events = self.supervisor.handle(request, Instant::now());
        self.events.extend(events);
    }

    /// Feeds reports and ticks until `done` holds or the timeout passes.
    fn until(&mut self, timeout: Duration, done: impl Fn(&[Event]) -> bool) -> bool {
        let end = Instant::now() + timeout;
        while Instant::now() < end {
            while let Ok(report) = self.reports.try_recv() {
                let events = self.supervisor.on_report(report, Instant::now());
                self.events.extend(events);
            }
            let events = self.supervisor.on_tick(Instant::now());
            self.events.extend(events);
            if done(&self.events) {
                return true;
            }
            sleep(Duration::from_millis(20));
        }
        false
    }

    fn output(&self, id: &ProcessId) -> String {
        self.events
            .iter()
            .filter_map(|e| match e {
                Event::Output { id: of, lines } if of == id => Some(lines.join("\n")),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn pid(&self, id: &ProcessId) -> Option<u32> {
        self.events.iter().rev().find_map(|e| match e {
            Event::Snapshot(s) => s
                .projects
                .iter()
                .flat_map(|p| &p.processes)
                .find(|p| &p.id == id)
                .map(|p| p.pid),
            _ => None,
        })?
    }
}

fn reached(id: &ProcessId, state: ProcessState) -> impl Fn(&[Event]) -> bool + '_ {
    move |events| {
        events
            .iter()
            .any(|e| matches!(e, Event::State { id: of, state: s } if of == id && *s == state))
    }
}

fn group_alive(pid: u32) -> bool {
    pty::signal_group(pid, nix::sys::signal::Signal::SIGCONT).is_ok()
}

#[test]
fn start_shows_output_and_stop_ends_the_whole_group() {
    let mut h = harness("app: echo hello from paddock; sleep 30 & sleep 30\n");
    let app = h.id("app");
    h.send(Request::Start(app.clone()));
    assert!(h.until(
        Duration::from_secs(10),
        reached(&app, ProcessState::Running)
    ));
    assert!(h.until(Duration::from_secs(10), |e| {
        e.iter().any(|e| matches!(e, Event::Output { lines, .. } if lines.iter().any(|l| l.contains("hello from paddock"))))
    }));
    let pid = h.pid(&app).expect("running process has a pid");
    assert!(group_alive(pid));

    h.send(Request::Stop(app.clone()));
    assert!(h.until(
        Duration::from_secs(10),
        reached(&app, ProcessState::Stopped)
    ));
    sleep(Duration::from_millis(200));
    assert!(!group_alive(pid), "the background sleep must be gone too");
}

#[test]
fn exit_codes_become_states() {
    let mut h = harness("ok: true\nbad: exit 3\n");
    let (ok, bad) = (h.id("ok"), h.id("bad"));
    h.send(Request::Start(ok.clone()));
    h.send(Request::Start(bad.clone()));
    assert!(h.until(Duration::from_secs(10), reached(&ok, ProcessState::Exited)));
    assert!(h.until(
        Duration::from_secs(10),
        reached(&bad, ProcessState::Crashed(Some(3)))
    ));
    assert!(h.output(&bad).contains("[exited with code 3]"));
}

#[test]
fn kill_is_immediate_even_when_sigterm_is_ignored() {
    let mut h = harness("stubborn: trap '' TERM; echo ready; sleep 60\n");
    let id = h.id("stubborn");
    h.send(Request::Start(id.clone()));
    assert!(h.until(Duration::from_secs(10), |e| {
        e.iter()
            .any(|e| matches!(e, Event::Output { lines, .. } if lines.iter().any(|l| l == "ready")))
    }));
    h.send(Request::Kill(id.clone()));
    assert!(h.until(
        Duration::from_secs(5),
        reached(&id, ProcessState::Crashed(None))
    ));
}

#[test]
fn port_override_reaches_the_process_and_is_saved() {
    let mut h = harness("web: echo port=$PORT; sleep 30\n");
    let web = h.id("web");
    h.send(Request::SetPort {
        id: web.clone(),
        port: 4567,
    });
    h.send(Request::Start(web.clone()));
    assert!(h.until(Duration::from_secs(10), |e| {
        e.iter().any(|e| matches!(e, Event::Output { lines, .. } if lines.iter().any(|l| l.contains("port=4567"))))
    }));
    let config = paddock::config::load_from(&h._dir.path().join("config.toml")).unwrap();
    let key = paddock::config::process_key(&h.project.canonicalize().unwrap(), "web");
    assert_eq!(config.processes[&key].port, Some(4567));
    h.send(Request::Stop(web.clone()));
    assert!(h.until(
        Duration::from_secs(10),
        reached(&web, ProcessState::Stopped)
    ));
}

#[test]
fn listening_ports_are_credited_to_their_process() {
    if ports::scan().is_err() {
        eprintln!("skipped: lsof is not available");
        return;
    }
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let mut h = harness(&format!(
        "srv: exec python3 -c \"import socket,time; s=socket.socket(); s.bind(('127.0.0.1',{port})); s.listen(); print('up', flush=True); time.sleep(30)\"\n"
    ));
    let srv = h.id("srv");
    h.send(Request::Start(srv.clone()));
    assert!(h.until(Duration::from_secs(10), |e| {
        e.iter()
            .any(|e| matches!(e, Event::Output { lines, .. } if lines.iter().any(|l| l == "up")))
    }));

    let mut stats = Stats::new();
    let mut credited = false;
    for _ in 0..20 {
        stats.refresh();
        let events = h.supervisor.apply_scan(&ports::scan().unwrap(), &stats);
        credited = events.iter().any(|e| {
            matches!(e, Event::Ports(ports)
            if ports.iter().any(|p| p.port == port && p.owner.as_ref() == Some(&srv)))
        });
        if credited {
            break;
        }
        sleep(Duration::from_millis(200));
    }
    assert!(credited, "port {port} was not credited to {srv}");
    h.send(Request::Stop(srv.clone()));
    assert!(h.until(
        Duration::from_secs(10),
        reached(&srv, ProcessState::Stopped)
    ));
}

#[test]
fn adding_a_project_saves_it_and_finds_its_processes() {
    let h = harness("a: true\nb: true\n");
    let config = paddock::config::load_from(&h._dir.path().join("config.toml")).unwrap();
    assert_eq!(config.projects, vec![h.project.canonicalize().unwrap()]);
    assert!(
        h.events
            .iter()
            .any(|e| matches!(e, Event::Notice(text) if text.contains("2 processes")))
    );
}

#[test]
fn moving_to_another_group_renames_and_persists() {
    let mut h = harness("api: true\n");
    h.send(Request::Move {
        id: h.id("api"),
        project: "backend".into(),
    });
    assert!(h.events.contains(&Event::Renamed {
        from: h.id("api"),
        to: ProcessId::new("backend", "api"),
    }));
    let config = paddock::config::load_from(&h._dir.path().join("config.toml")).unwrap();
    let key = paddock::config::process_key(&h.project.canonicalize().unwrap(), "api");
    assert_eq!(config.processes[&key].group.as_deref(), Some("backend"));
}

#[test]
fn a_missing_folder_is_refused() {
    let (tx, _rx) = mpsc::channel(8);
    let dir = tempfile::tempdir().unwrap();
    let mut supervisor = Supervisor::new(dir.path().join("config.toml"), tx);
    let events = supervisor.handle(
        Request::AddProject(Path::new("/no/such/folder").into()),
        Instant::now(),
    );
    assert!(matches!(&events[..], [Event::Notice(text)] if text.contains("does not exist")));
}
