//! Watches the dev servers running on this machine. Never starts anything.
//!
//! Every two seconds, on a blocking thread: `lsof` for listening ports,
//! `sysinfo` for the process table, then `servers::collect` groups them
//! into projects and servers. The TUI gets a full `Snapshot` each time.
//!
//! Requests from the TUI stop or kill what is already running, through
//! `actions`, which re-checks every pid before signalling it. A stopped
//! server that is still there after five seconds gets SIGKILL.
//!
//! Safety rules for everything in here: `.claude/rules/process-safety.md`.

pub mod actions;
pub mod ports;
pub mod servers;
pub mod stats;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Duration;

use nix::sys::signal::Signal;
use tokio::sync::mpsc;
use tokio::time::{self, Instant};

use crate::ipc::protocol::{Event, Request};
use crate::ipc::transport::ServerEnd;
use crate::model::{ServerId, Snapshot};
use stats::Stats;

const SCAN: Duration = Duration::from_secs(2);
const STOP_GRACE: Duration = Duration::from_secs(5);

/// One scan: refresh the process table, read the ports, group them.
pub fn scan(
    stats: &mut Stats,
    home: &std::path::Path,
    stopping: &HashSet<ServerId>,
) -> Result<Snapshot, String> {
    stats.refresh();
    let listeners = ports::scan()?;
    let protected: HashSet<u32> = stats.own_line().into_iter().collect();
    Ok(servers::collect(
        &listeners, stats, home, stopping, &protected,
    ))
}

pub fn home() -> Option<PathBuf> {
    let home = PathBuf::from(std::env::var_os("HOME")?);
    Some(home.canonicalize().unwrap_or(home))
}

/// Runs until the TUI hangs up.
pub async fn run(mut server: ServerEnd) {
    let Some(home) = home() else {
        let _ = server
            .events
            .send(Event::Notice(
                "HOME is not set, so Paddock cannot tell projects apart.".into(),
            ))
            .await;
        return;
    };
    let mut timer = time::interval(SCAN);
    let (scan_tx, mut scan_rx) = mpsc::channel::<(Stats, Result<Snapshot, String>)>(1);
    let mut stats = Some(Stats::new());
    let mut stopping: HashMap<ServerId, Instant> = HashMap::new();
    let mut lsof_error_shown = false;

    loop {
        let events = tokio::select! {
            request = server.requests.recv() => match request {
                Some(request) => {
                    let events = tokio::task::block_in_place(|| handle(request, &mut stopping));
                    timer.reset_immediately();
                    events
                }
                None => return,
            },
            _ = timer.tick() => {
                if let Some(mut taken) = stats.take() {
                    let tx = scan_tx.clone();
                    let home = home.clone();
                    let marked: HashSet<ServerId> = stopping.keys().copied().collect();
                    tokio::task::spawn_blocking(move || {
                        let result = scan(&mut taken, &home, &marked);
                        let _ = tx.blocking_send((taken, result));
                    });
                }
                Vec::new()
            }
            Some((returned, result)) = scan_rx.recv() => {
                stats = Some(returned);
                match result {
                    Ok(snapshot) => {
                        let mut events = tokio::task::block_in_place(|| escalate(&snapshot, &mut stopping));
                        events.insert(0, Event::Snapshot(snapshot));
                        events
                    }
                    Err(err) if !lsof_error_shown => {
                        lsof_error_shown = true;
                        vec![Event::Notice(err)]
                    }
                    Err(_) => Vec::new(),
                }
            }
        };
        for event in events {
            if server.events.send(event).await.is_err() {
                return;
            }
        }
    }
}

fn handle(request: Request, stopping: &mut HashMap<ServerId, Instant>) -> Vec<Event> {
    let notice = match request {
        Request::Stop(id) => match actions::signal_server(id, Signal::SIGTERM) {
            Ok(_) => {
                stopping.insert(id, Instant::now() + STOP_GRACE);
                "Asked the server to stop. It gets 5 seconds before it is killed.".to_owned()
            }
            Err(err) => format!("Could not stop it: {err}."),
        },
        Request::Kill(id) => match actions::signal_server(id, Signal::SIGKILL) {
            Ok(n) => {
                stopping.remove(&id);
                format!("Killed {n} process{}.", if n == 1 { "" } else { "es" })
            }
            Err(err) => format!("Could not kill it: {err}."),
        },
        Request::KillPort { port, pid } => match actions::stop_listener(port, pid) {
            Ok(command) => {
                format!("Asked {command} (pid {pid}) to quit. Port {port} frees up in a moment.")
            }
            Err(err) => format!("Could not stop it: {err}."),
        },
    };
    vec![Event::Notice(notice)]
}

/// Servers that ignored SIGTERM for too long get SIGKILL. Servers that are
/// gone are forgotten.
fn escalate(snapshot: &Snapshot, stopping: &mut HashMap<ServerId, Instant>) -> Vec<Event> {
    let alive: HashSet<ServerId> = snapshot.servers().map(|s| s.id).collect();
    stopping.retain(|id, _| alive.contains(id) || actions::is_running(*id));
    let now = Instant::now();
    let due: Vec<ServerId> = stopping
        .iter()
        .filter(|(_, deadline)| now >= **deadline)
        .map(|(id, _)| *id)
        .collect();
    let mut events = Vec::new();
    for id in due {
        stopping.remove(&id);
        if actions::signal_server(id, Signal::SIGKILL).is_ok() {
            events.push(Event::Notice(
                "The server did not stop within 5 seconds, so it was killed.".into(),
            ));
        }
    }
    events
}
