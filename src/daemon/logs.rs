//! Turns a process's raw output bytes into lines.
//!
//! Output arrives in arbitrary chunks: a line can be split across reads, and
//! so can a multi-byte UTF-8 character. `LineSplitter` keeps the unfinished
//! tail until its newline arrives. A prompt that never ends in a newline
//! ("press i to open iOS") is flushed after a short wait so it still shows.
//!
//! Lines keep their ANSI codes and `\r` redraws; the TUI interprets them.
//! Nothing here is written to disk, and nothing reads environment variables.

use std::time::{Duration, Instant};

/// How long an unfinished line may wait for its newline before it is shown.
pub const PARTIAL_WAIT: Duration = Duration::from_millis(250);
/// A single line longer than this is cut, so one runaway line cannot grow
/// without bound.
const MAX_LINE: usize = 16 * 1024;

#[derive(Debug, Default)]
pub struct LineSplitter {
    partial: Vec<u8>,
    since: Option<Instant>,
}

impl LineSplitter {
    pub fn push(&mut self, bytes: &[u8], now: Instant) -> Vec<String> {
        let mut lines = Vec::new();
        for chunk in bytes.split_inclusive(|b| *b == b'\n') {
            self.partial.extend_from_slice(chunk);
            if chunk.ends_with(b"\n") || self.partial.len() >= MAX_LINE {
                lines.push(self.take());
            }
        }
        if !self.partial.is_empty() && self.since.is_none() {
            self.since = Some(now);
        }
        lines
    }

    /// The unfinished line, if it has waited long enough.
    pub fn flush_stale(&mut self, now: Instant) -> Option<String> {
        let since = self.since?;
        (now.duration_since(since) >= PARTIAL_WAIT).then(|| self.take())
    }

    /// Whatever is left, when the process ends.
    pub fn finish(&mut self) -> Option<String> {
        (!self.partial.is_empty()).then(|| self.take())
    }

    fn take(&mut self) -> String {
        self.since = None;
        let bytes = std::mem::take(&mut self.partial);
        let text = String::from_utf8_lossy(&bytes);
        text.trim_end_matches(['\n', '\r']).to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_split_across_chunks_are_joined() {
        let mut splitter = LineSplitter::default();
        let now = Instant::now();
        assert!(splitter.push(b"GET /pro", now).is_empty());
        assert_eq!(
            splitter.push(b"ducts 200\r\nnext", now),
            vec!["GET /products 200"]
        );
        assert_eq!(splitter.finish().as_deref(), Some("next"));
    }

    #[test]
    fn utf8_split_across_chunks_survives() {
        let mut splitter = LineSplitter::default();
        let check = "✓ ready\n".as_bytes();
        let now = Instant::now();
        assert!(splitter.push(&check[..1], now).is_empty());
        assert_eq!(splitter.push(&check[1..], now), vec!["✓ ready"]);
    }

    #[test]
    fn a_prompt_without_newline_is_flushed_after_a_wait() {
        let mut splitter = LineSplitter::default();
        let start = Instant::now();
        splitter.push(b"Press i to open iOS", start);
        assert_eq!(splitter.flush_stale(start), None);
        assert_eq!(
            splitter.flush_stale(start + PARTIAL_WAIT).as_deref(),
            Some("Press i to open iOS")
        );
        assert_eq!(splitter.finish(), None);
    }

    #[test]
    fn carriage_returns_inside_a_line_are_kept_for_the_tui() {
        let mut splitter = LineSplitter::default();
        let lines = splitter.push(b"10%\r55%\r100%\n", Instant::now());
        assert_eq!(lines, vec!["10%\r55%\r100%"]);
    }
}
