//! Watches the dev servers running on this machine. Never starts anything.
//!
//! Every two seconds, on a blocking thread: `lsof` for listening ports,
//! `sysinfo` for the process table, then `servers::collect` groups them
//! into projects and servers. The TUI gets a full `Snapshot` each time.
//!
//! The TUI follows one server at a time. If that server was started through
//! `paddock run`, its log file is read every 250 ms and new lines are sent
//! as `Event::Logs`.
//!
//! Requests from the TUI stop or kill what is already running, through
//! `actions`, which re-checks every pid before signalling it. A stopped
//! server that is still there after five seconds gets SIGKILL.
//!
//! Safety rules for everything in here: `.claude/rules/process-safety.md`.

pub mod actions;
pub mod follow;
pub mod logs;
pub mod ports;
pub mod servers;
pub mod stats;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::sync::mpsc;
use tokio::time::{self, Instant};

use crate::capture;
use crate::ipc::protocol::{Event, Request};
use crate::ipc::transport::ServerEnd;
use crate::model::{ServerId, Snapshot};
use actions::How;
use follow::LogFollower;
use stats::Stats;

const SCAN: Duration = Duration::from_secs(2);
const LOG_POLL: Duration = Duration::from_millis(250);
const STOP_GRACE: Duration = Duration::from_secs(5);

/// One scan: refresh the process table, read the ports, group them.
/// `logs_dir` is where `paddock run` writes logs (`capture::logs_dir`).
pub fn scan(
    stats: &mut Stats,
    home: &Path,
    logs_dir: Option<&Path>,
    stopping: &HashSet<ServerId>,
) -> Result<Snapshot, String> {
    stats.refresh();
    let listeners = ports::scan()?;
    let protected: HashSet<u32> = stats.own_line().into_iter().collect();
    let cx = servers::Context {
        home,
        logs_dir,
        stopping,
        protected: &protected,
    };
    Ok(servers::collect(&listeners, stats, &cx))
}

/// The user's home folder: `HOME`, or `USERPROFILE` on Windows.
pub fn home() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .filter(|h| !h.is_empty())
        .map(PathBuf::from)?;
    // On Windows canonicalize adds a \\?\ prefix the process table does not
    // use, so only resolve links on Unix.
    if cfg!(unix) {
        return Some(home.canonicalize().unwrap_or(home));
    }
    Some(home)
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
    let mut log_timer = time::interval(LOG_POLL);
    let (scan_tx, mut scan_rx) = mpsc::channel::<(Stats, Result<Snapshot, String>)>(1);
    let mut stats = Some(Stats::new());
    let mut stopping: HashMap<ServerId, Instant> = HashMap::new();
    let mut last = Snapshot::default();
    let mut wanted: Option<ServerId> = None;
    let mut follower: Option<LogFollower> = None;
    let mut lsof_error_shown = false;

    loop {
        let events = tokio::select! {
            request = server.requests.recv() => match request {
                Some(Request::Follow(id)) => {
                    wanted = id;
                    follower = None;
                    tokio::task::block_in_place(|| start_following(&last, wanted, &mut follower))
                }
                Some(request) => {
                    let events = tokio::task::block_in_place(|| handle(request, &mut stopping));
                    timer.reset_immediately();
                    events
                }
                None => return,
            },
            _ = log_timer.tick(), if follower.is_some() => match &mut follower {
                Some(f) => tokio::task::block_in_place(|| f.poll()).into_iter().collect(),
                None => Vec::new(),
            },
            _ = timer.tick() => {
                if let Some(mut taken) = stats.take() {
                    let tx = scan_tx.clone();
                    let home = home.clone();
                    let marked: HashSet<ServerId> = stopping.keys().copied().collect();
                    tokio::task::spawn_blocking(move || {
                        let logs_dir = capture::logs_dir();
                        let result = scan(&mut taken, &home, logs_dir.as_deref(), &marked);
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
                        last = snapshot.clone();
                        // The followed server may have just appeared, or got a log.
                        if follower.is_none() {
                            events.extend(tokio::task::block_in_place(|| {
                                start_following(&last, wanted, &mut follower)
                            }));
                        }
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

/// Opens the log of the wanted server, if it has one, and sends its tail.
fn start_following(
    snapshot: &Snapshot,
    wanted: Option<ServerId>,
    follower: &mut Option<LogFollower>,
) -> Vec<Event> {
    let Some(id) = wanted else {
        return Vec::new();
    };
    let Some(path) = snapshot
        .servers()
        .find(|s| s.id == id)
        .and_then(|s| s.log.clone())
    else {
        return Vec::new();
    };
    let mut started = LogFollower::new(id, path);
    let events = started.poll().into_iter().collect();
    *follower = Some(started);
    events
}

fn handle(request: Request, stopping: &mut HashMap<ServerId, Instant>) -> Vec<Event> {
    let notice = match request {
        Request::Stop(id) => match actions::signal_server(id, How::Stop) {
            Ok(_) => {
                stopping.insert(id, Instant::now() + STOP_GRACE);
                "Asked the server to stop. It gets 5 seconds before it is killed.".to_owned()
            }
            Err(err) => format!("Could not stop it: {err}."),
        },
        Request::Kill(id) => match actions::signal_server(id, How::Kill) {
            Ok(n) => {
                stopping.remove(&id);
                format!("Killed {n} process{}.", if n == 1 { "" } else { "es" })
            }
            Err(err) => format!("Could not kill it: {err}."),
        },
        Request::Follow(_) => return Vec::new(),
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
        if actions::signal_server(id, How::Kill).is_ok() {
            events.push(Event::Notice(
                "The server did not stop within 5 seconds, so it was killed.".into(),
            ));
        }
    }
    events
}
