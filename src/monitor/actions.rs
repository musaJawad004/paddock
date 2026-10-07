//! Stopping things the user asked to stop. The only code in Paddock that
//! ends processes.
//!
//! Every action re-checks the process table first: the pid must still
//! exist and still have the start time the user saw, so a reused pid is
//! never hit. Paddock itself and the processes above it (the shell and
//! terminal it runs in) are never touched, and neither is pid 1.
//! Children go before parents, so a parent that respawns a dead child has
//! already been told to stop.
//!
//! Stop is SIGTERM on Unix. Windows has no polite stop for another
//! process, so there both stop and kill end it at once.

use super::servers::ProcessTable;
use super::stats::Stats;
use crate::model::ServerId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum How {
    /// SIGTERM: the server may clean up.
    Stop,
    /// SIGKILL: now.
    Kill,
}

/// Stops or kills every process in the server's tree. Returns how many
/// processes were reached.
pub fn signal_server(id: ServerId, how: How) -> Result<usize, String> {
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
    let sent = tree
        .iter()
        .rev()
        .filter(|pid| match how {
            How::Stop => stats.terminate(**pid),
            How::Kill => stats.kill(**pid),
        })
        .count();
    if sent == 0 {
        return Err("no process accepted the signal".into());
    }
    Ok(sent)
}

/// True while the server's top process is still there (same start time).
pub fn is_running(id: ServerId) -> bool {
    let mut stats = Stats::new();
    stats.refresh();
    stats.start_time(id.pid) == Some(id.started) && stats.is_alive(id.pid)
}
