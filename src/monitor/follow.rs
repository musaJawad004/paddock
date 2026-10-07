//! Reads a captured log file as it grows.
//!
//! The first read starts near the end (the last 256 KB, from the first
//! whole line), so a log that ran for days opens instantly. Later reads
//! take only what was appended. A file that got shorter (replaced or
//! truncated) is read again from the start.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::time::Instant;

use super::logs::LineSplitter;
use crate::ipc::protocol::Event;
use crate::model::ServerId;

const FIRST_READ: u64 = 256 * 1024;
/// More than this in one poll is sent in the next poll.
const MAX_READ: u64 = 1024 * 1024;

pub struct LogFollower {
    id: ServerId,
    path: PathBuf,
    offset: Option<u64>,
    splitter: LineSplitter,
}

impl LogFollower {
    pub fn new(id: ServerId, path: PathBuf) -> Self {
        Self {
            id,
            path,
            offset: None,
            splitter: LineSplitter::default(),
        }
    }

    /// New lines since the last call, if any.
    pub fn poll(&mut self) -> Option<Event> {
        let mut file = File::open(&self.path).ok()?;
        let len = file.metadata().ok()?.len();
        let (start, reset) = match self.offset {
            None => (len.saturating_sub(FIRST_READ), true),
            Some(offset) if len < offset => (0, true),
            Some(offset) => (offset, false),
        };
        if reset {
            self.splitter = LineSplitter::default();
        }
        let end = len.min(start + MAX_READ);
        file.seek(SeekFrom::Start(start)).ok()?;
        let mut bytes = Vec::with_capacity((end - start) as usize);
        file.take(end - start).read_to_end(&mut bytes).ok()?;
        self.offset = Some(start + bytes.len() as u64);

        // Starting mid-file: drop the partial first line.
        let bytes = if reset && start > 0 {
            match bytes.iter().position(|b| *b == b'\n') {
                Some(i) => &bytes[i + 1..],
                None => &bytes[..0],
            }
        } else {
            &bytes[..]
        };
        let now = Instant::now();
        let mut lines = self.splitter.push(bytes, now);
        lines.extend(self.splitter.flush_stale(now));
        if lines.is_empty() && !reset {
            return None;
        }
        Some(Event::Logs {
            id: self.id,
            lines,
            reset,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Write;

    use super::*;

    fn lines(event: Option<Event>) -> (Vec<String>, bool) {
        match event {
            Some(Event::Logs { lines, reset, .. }) => (lines, reset),
            other => panic!("expected logs, got {other:?}"),
        }
    }

    #[test]
    fn follows_appends_and_restarts_after_truncation() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.log");
        fs::write(&path, "one\r\ntwo\n").unwrap();
        let id = ServerId { pid: 1, started: 1 };
        let mut follower = LogFollower::new(id, path.clone());

        assert_eq!(
            lines(follower.poll()),
            (vec!["one".into(), "two".into()], true)
        );
        assert!(follower.poll().is_none(), "nothing new");

        let mut file = fs::OpenOptions::new().append(true).open(&path).unwrap();
        file.write_all(b"three\n").unwrap();
        assert_eq!(lines(follower.poll()), (vec!["three".into()], false));

        fs::write(&path, "fresh\n").unwrap();
        assert_eq!(lines(follower.poll()), (vec!["fresh".into()], true));
    }

    #[test]
    fn a_long_log_opens_at_its_tail() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("big.log");
        let line = "x".repeat(99) + "\n";
        fs::write(&path, line.repeat(5000)).unwrap();
        let mut follower = LogFollower::new(ServerId { pid: 1, started: 1 }, path);
        let (got, reset) = lines(follower.poll());
        assert!(reset);
        assert!(got.len() < 5000 && got.len() > 2000, "{}", got.len());
        assert!(got.iter().all(|l| l.len() == 99), "no partial first line");
    }

    #[test]
    fn a_missing_file_is_quiet() {
        let mut follower = LogFollower::new(ServerId { pid: 1, started: 1 }, "/no/such.log".into());
        assert!(follower.poll().is_none());
    }
}
