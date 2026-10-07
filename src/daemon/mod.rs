//! The side of Paddock that owns child processes.
//!
//! `run` is driven only by `ipc::Request` and reports only through
//! `ipc::Event`. Today it runs as a task inside the `paddock` process, so
//! quitting the TUI stops every process (the TUI asks first). v0.2 moves the
//! same code into `paddock daemon` so servers keep running after the TUI
//! quits.
//!
//! The loop: requests from the TUI, reports from child processes, a 200 ms
//! tick (unfinished lines, stop timeouts) and a 2 s scan (ports with lsof,
//! CPU and memory with sysinfo) that runs on a blocking thread.
//!
//! Safety rules for everything in here: `.claude/rules/process-safety.md`.

pub mod logs;
pub mod ports;
pub mod pty;
pub mod stats;
pub mod supervisor;

use std::path::PathBuf;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio::time::{self, Instant};

use crate::config::Config;
use crate::ipc::protocol::Event;
use crate::ipc::transport::ServerEnd;
use stats::Stats;
use supervisor::Supervisor;

const TICK: Duration = Duration::from_millis(200);
const SCAN: Duration = Duration::from_secs(2);
const SHUTDOWN_WAIT: Duration = Duration::from_secs(3);

type ScanResult = (Stats, Result<Vec<ports::Listener>, String>);

/// Runs until the TUI hangs up, then stops every process it started.
pub async fn run(mut server: ServerEnd, config: Config, config_path: PathBuf) {
    let (reports_tx, mut reports) = mpsc::channel(1024);
    let mut supervisor = Supervisor::new(config_path, reports_tx);
    tokio::task::block_in_place(|| supervisor.load(&config));

    let mut opening = vec![Event::Snapshot(supervisor.snapshot())];
    opening.extend(supervisor.history());
    if send_all(&server.events, opening).await.is_err() {
        return;
    }

    let mut tick = time::interval(TICK);
    let mut scan_timer = time::interval(SCAN);
    let (scan_tx, mut scan_rx) = mpsc::channel::<ScanResult>(1);
    let mut stats = Some(Stats::new());
    let mut lsof_error_shown = false;

    loop {
        let events = tokio::select! {
            request = server.requests.recv() => match request {
                Some(request) => {
                    let now = Instant::now().into_std();
                    tokio::task::block_in_place(|| supervisor.handle(request, now))
                }
                None => break,
            },
            Some(report) = reports.recv() => supervisor.on_report(report, Instant::now().into_std()),
            _ = tick.tick() => {
                if supervisor.wants_scan && stats.is_some() {
                    scan_timer.reset_immediately();
                }
                supervisor.on_tick(Instant::now().into_std())
            }
            _ = scan_timer.tick() => {
                if let Some(mut taken) = stats.take() {
                    supervisor.wants_scan = false;
                    let tx = scan_tx.clone();
                    tokio::task::spawn_blocking(move || {
                        taken.refresh();
                        let listeners = ports::scan();
                        let _ = tx.blocking_send((taken, listeners));
                    });
                }
                Vec::new()
            }
            Some((returned, listeners)) = scan_rx.recv() => {
                let events = match listeners {
                    Ok(listeners) => supervisor.apply_scan(&listeners, &returned),
                    Err(err) if !lsof_error_shown => {
                        lsof_error_shown = true;
                        vec![Event::Notice(err)]
                    }
                    Err(_) => Vec::new(),
                };
                stats = Some(returned);
                events
            }
        };
        if send_all(&server.events, events).await.is_err() {
            break;
        }
    }
    shutdown(&mut supervisor, &mut reports).await;
}

/// SIGTERM everything, wait up to three seconds for exits, then SIGKILL
/// whatever is left.
async fn shutdown(supervisor: &mut Supervisor, reports: &mut mpsc::Receiver<pty::Tagged>) {
    if supervisor.stop_all(Instant::now().into_std()) == 0 {
        return;
    }
    let deadline = Instant::now() + SHUTDOWN_WAIT;
    while supervisor.running() > 0 {
        match time::timeout_at(deadline, reports.recv()).await {
            Ok(Some(report)) => {
                supervisor.on_report(report, Instant::now().into_std());
            }
            Ok(None) | Err(_) => break,
        }
    }
    supervisor.kill_all();
}

async fn send_all(tx: &mpsc::Sender<Event>, events: Vec<Event>) -> Result<(), ()> {
    for event in events {
        tx.send(event).await.map_err(|_| ())?;
    }
    Ok(())
}
