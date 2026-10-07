//! The monitor against real processes: small Python servers started by the
//! test (standing in for "a terminal"), never through Paddock. Each test
//! cleans up its own processes, also when it fails.
//!
//! Socket use is allowed here (and only here): see testing-and-verification.

// Test helpers may panic: a failed setup should fail the test loudly.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::thread::sleep;
use std::time::{Duration, Instant};

use nix::sys::signal::Signal;
use paddock::model::{Server, ServerId};
use paddock::monitor::servers::ProcessTable;
use paddock::monitor::stats::Stats;
use paddock::monitor::{actions, ports, scan};

/// Kills what it holds when dropped, so a failing test leaves nothing behind.
struct Spawned(Vec<Child>);

impl Drop for Spawned {
    fn drop(&mut self) {
        for child in &mut self.0 {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

fn listen_script(port: u16) -> String {
    format!(
        "import socket,time; s=socket.socket(); s.bind(('127.0.0.1',{port})); s.listen(); time.sleep(60)"
    )
}

/// A temp "home" with a node project in it.
fn project() -> (tempfile::TempDir, PathBuf) {
    let home = tempfile::tempdir().unwrap();
    let dir = home.path().join("shop");
    fs::create_dir(&dir).unwrap();
    fs::write(dir.join("package.json"), "{}").unwrap();
    (home, dir)
}

fn canonical(path: &Path) -> PathBuf {
    path.canonicalize().unwrap()
}

/// Scans until a server holds `port`, or gives up after five seconds.
fn find(home: &Path, port: u16) -> Option<(String, Server)> {
    let mut stats = Stats::new();
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        let snapshot = scan(&mut stats, &canonical(home), &HashSet::new()).unwrap();
        for project in &snapshot.projects {
            if let Some(server) = project.servers.iter().find(|s| s.ports.contains(&port)) {
                return Some((project.name.clone(), server.clone()));
            }
        }
        sleep(Duration::from_millis(150));
    }
    None
}

/// Waits up to five seconds for `pid` to disappear.
fn gone(pid: u32) -> bool {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if actions::send(pid, Signal::SIGCONT).is_err() {
            return true;
        }
        sleep(Duration::from_millis(50));
    }
    false
}

fn lsof_missing() -> bool {
    if ports::scan().is_err() {
        eprintln!("skipped: lsof is not available");
        return true;
    }
    false
}

#[test]
fn a_server_started_elsewhere_is_found_and_grouped_by_project() {
    if lsof_missing() {
        return;
    }
    let (home, dir) = project();
    let port = free_port();
    let child = Command::new("python3")
        .args(["-c", &listen_script(port)])
        .current_dir(&dir)
        .spawn()
        .unwrap();
    let pid = child.id();
    let _guard = Spawned(vec![child]);

    let (project, server) = find(home.path(), port).expect("server not found");
    assert_eq!(project, "shop");
    assert_eq!(server.id.pid, pid);
    assert_eq!(server.cwd, canonical(&dir));
    assert!(server.usage.memory_bytes > 0);
}

#[test]
fn stop_ends_the_whole_tree() {
    if lsof_missing() {
        return;
    }
    let (home, dir) = project();
    let port = free_port();
    // A parent that starts the listener as its child, like npm starting vite.
    let parent = Command::new("python3")
        .args([
            "-c",
            &format!(
                "import subprocess,sys,time; subprocess.Popen([sys.executable,'-c',{:?}]); time.sleep(60)",
                listen_script(port)
            ),
        ])
        .current_dir(&dir)
        .spawn()
        .unwrap();
    let parent_pid = parent.id();
    let mut guard = Spawned(vec![parent]);

    let (_, server) = find(home.path(), port).expect("server not found");
    assert_eq!(
        server.id.pid, parent_pid,
        "the parent is the top of the server"
    );
    assert_eq!(server.processes, 2);
    let listener_pid = ports::scan()
        .unwrap()
        .into_iter()
        .find(|l| l.port == port)
        .map(|l| l.pid)
        .unwrap();

    let sent = actions::signal_server(server.id, Signal::SIGTERM).unwrap();
    assert_eq!(sent, 2);
    assert!(gone(listener_pid), "the child listener must stop too");
    // The parent is our child, so we must reap it (a shell would).
    let status = guard.0[0].wait().unwrap();
    assert!(!status.success(), "it ended by SIGTERM");
}

#[test]
fn a_reused_pid_is_never_signalled() {
    let child = Command::new("sleep").arg("30").spawn().unwrap();
    let pid = child.id();
    let _guard = Spawned(vec![child]);
    let stale = ServerId { pid, started: 1 };
    let err = actions::signal_server(stale, Signal::SIGTERM).unwrap_err();
    assert!(err.contains("another program"), "{err}");
    sleep(Duration::from_millis(200));
    assert!(
        actions::send(pid, Signal::SIGCONT).is_ok(),
        "sleep must still be running"
    );
}

#[test]
fn paddock_never_signals_its_own_process() {
    let mut stats = Stats::new();
    stats.refresh();
    let me = std::process::id();
    let started = stats.start_time(me).unwrap();
    let result = actions::signal_server(ServerId { pid: me, started }, Signal::SIGCONT);
    let err = result.expect_err("own process must be protected");
    assert!(err.contains("Paddock itself"), "{err}");
}
