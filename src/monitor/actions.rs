//! Stopping things the user asked to stop. The only code in Paddock that
//! sends signals.
//!
//! Every action re-checks the process table first: the pid must still
//! exist and still have the start time the TUI saw, so a reused pid is
//! never signalled. Paddock itself and the processes above it (the shell and
//! terminal it runs in) are never signalled, and neither is pid 1.
//! Children are signalled before parents, so a parent that respawns a dead
//! child has already got its own signal.

use nix::sys::signal::{self, Signal};
use nix::unistd::Pid;

use super::ports;
use super::servers::ProcessTable;
use super::stats::Stats;
use crate::model::ServerId;

/// Sends `signal` to every process in the server's tree. Returns how many
/// processes got it.
pub fn signal_server(id: ServerId, signal: Signal) -> Result<usize, String> {
    let mut stats = Stats::new();
    stats.refresh();
    match stats.start_time(id.pid) {
        None => return Err("it has already stopped".into()),
        Some(started) if started != id.started => {
            return Err("its pid now belongs to another program, so nothing was sent".into());
        }
        Some(_) => {}
    }
    let protected = stats.own_line();
    if protected.contains(&id.pid) {
        return Err("that is Paddock itself or the terminal it runs in".into());
    }
    let tree: Vec<u32> = stats
        .tree(id.pid)
        .into_iter()
        .filter(|p| *p > 1 && !protected.contains(p))
        .collect();
    let mut sent = 0;
    for pid in tree.iter().rev() {
        if send(*pid, signal).is_ok() {
            sent += 1;
        }
    }
    if sent == 0 {
        return Err("no process accepted the signal".into());
    }
    Ok(sent)
}

/// True while the server's top process is still there (same start time).
pub fn is_running(id: ServerId) -> bool {
    let mut stats = Stats::new();
    stats.refresh();
    stats.start_time(id.pid) == Some(id.started)
}

/// SIGTERM to a listener that is not one of the user's servers, after a
/// fresh lsof shows it still holds the port.
pub fn stop_listener(port: u16, pid: u32) -> Result<String, String> {
    let listeners = ports::scan()?;
    let Some(listener) = listeners.iter().find(|l| l.port == port && l.pid == pid) else {
        return Err(format!("pid {pid} no longer listens on port {port}"));
    };
    let mut stats = Stats::new();
    stats.refresh();
    if pid <= 1 || stats.own_line().contains(&pid) {
        return Err(format!("refusing to stop pid {pid}"));
    }
    send(pid, Signal::SIGTERM).map_err(|err| match err {
        nix::errno::Errno::EPERM => {
            format!("not allowed to stop pid {pid}: it belongs to another user")
        }
        other => format!("could not stop pid {pid}: {other}"),
    })?;
    Ok(listener.command.clone())
}

pub fn send(pid: u32, signal: Signal) -> nix::Result<()> {
    let raw = i32::try_from(pid).map_err(|_| nix::errno::Errno::EINVAL)?;
    if raw <= 1 {
        return Err(nix::errno::Errno::EINVAL);
    }
    signal::kill(Pid::from_raw(raw), signal)
}
