//! Spawns one command in its own pseudo-terminal via `portable-pty`, so dev
//! servers see a real terminal and keep their colours, spinners and QR
//! codes.
//!
//! The command runs through the user's login shell (`$SHELL -lc <cmd>`), so
//! PATH and version managers work as they do in a terminal. The command text
//! comes only from project files, paddock.toml, or a port the user typed
//! (digits only); nothing else is spliced into it.
//!
//! portable-pty starts the child with `setsid`, so it leads a new session and
//! process group whose id is its pid. `signal_group` uses that to reach the
//! whole tree. Output is read on a blocking thread and sent as raw chunks;
//! another thread waits for the exit status.

use std::io::{self, Read};
use std::path::Path;
use std::thread;

use nix::sys::signal::{Signal, killpg};
use nix::unistd::Pid;
use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};
use tokio::sync::mpsc;

/// Wide enough that servers do not wrap their own lines; the TUI clips.
const SIZE: PtySize = PtySize {
    rows: 40,
    cols: 160,
    pixel_width: 0,
    pixel_height: 0,
};

/// A report from the threads of one run. `run` tells runs apart, so a late
/// report from a process that was restarted is recognised and dropped.
#[derive(Debug)]
pub struct Tagged {
    pub run: u64,
    pub report: Report,
}

#[derive(Debug)]
pub enum Report {
    Bytes(Vec<u8>),
    /// `None` when the process was ended by a signal.
    Exited(Option<i32>),
}

/// A running child. Dropping it closes the PTY.
pub struct Running {
    pub pid: u32,
    /// Kept so the PTY stays open while the child runs; also the future
    /// input path for attach mode.
    _master: Box<dyn MasterPty + Send>,
}

pub fn spawn(
    command: &str,
    cwd: &Path,
    env: &[(String, String)],
    run: u64,
    reports: mpsc::Sender<Tagged>,
) -> io::Result<Running> {
    let shell = std::env::var("SHELL")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "/bin/sh".to_owned());
    let mut builder = CommandBuilder::new(shell);
    builder.arg("-lc");
    builder.arg(command);
    builder.cwd(cwd);
    builder.env("TERM", "xterm-256color");
    for (key, value) in env {
        builder.env(key, value);
    }

    let pair = native_pty_system()
        .openpty(SIZE)
        .map_err(|e| io::Error::other(e.to_string()))?;
    let mut child = pair
        .slave
        .spawn_command(builder)
        .map_err(|e| io::Error::other(e.to_string()))?;
    drop(pair.slave);
    let pid = child
        .process_id()
        .ok_or_else(|| io::Error::other("the child has no pid"))?;
    let mut reader = pair
        .master
        .try_clone_reader()
        .map_err(|e| io::Error::other(e.to_string()))?;

    let output = reports.clone();
    thread::Builder::new()
        .name(format!("pty-read-{pid}"))
        .spawn(move || {
            let mut buf = [0u8; 8192];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        let report = Report::Bytes(buf[..n].to_vec());
                        if output.blocking_send(Tagged { run, report }).is_err() {
                            break;
                        }
                    }
                }
            }
        })?;
    thread::Builder::new()
        .name(format!("pty-wait-{pid}"))
        .spawn(move || {
            let code = match child.wait() {
                Ok(status) if status.signal().is_some() => None,
                Ok(status) => Some(status.exit_code() as i32),
                Err(_) => None,
            };
            let _ = reports.blocking_send(Tagged {
                run,
                report: Report::Exited(code),
            });
        })?;

    Ok(Running {
        pid,
        _master: pair.master,
    })
}

/// Sends `signal` to the process group led by `pid`. Only call this for a
/// child Paddock spawned and that has not been reaped yet, or right after
/// it exits to clean up what it left behind.
pub fn signal_group(pid: u32, signal: Signal) -> nix::Result<()> {
    let Ok(raw) = i32::try_from(pid) else {
        return Err(nix::errno::Errno::EINVAL);
    };
    if raw <= 1 {
        return Err(nix::errno::Errno::EINVAL);
    }
    killpg(Pid::from_raw(raw), signal)
}
