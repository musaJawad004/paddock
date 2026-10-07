//! Puts text on the system clipboard.
//!
//! Tries the platform's copy command (pbcopy on macOS; wl-copy, xclip or
//! xsel on Linux), passing the text on stdin with fixed arguments and no
//! shell. If none is available, the caller falls back to OSC 52, an escape
//! sequence most terminals (and tmux, and SSH sessions) turn into a
//! clipboard write.
//!
//! Blocking: run it off the TUI loop, e.g. with `spawn_blocking`.

use std::io::Write;
use std::process::{Command, Stdio};

use base64_lite::encode;

#[cfg(target_os = "macos")]
const COMMANDS: &[(&str, &[&str])] = &[("pbcopy", &[])];

#[cfg(not(target_os = "macos"))]
const COMMANDS: &[(&str, &[&str])] = &[
    ("wl-copy", &[]),
    ("xclip", &["-selection", "clipboard"]),
    ("xsel", &["--clipboard", "--input"]),
];

pub enum Copied {
    /// A clipboard command took the text.
    System,
    /// No command worked; the caller should write this OSC 52 sequence.
    Osc52(String),
}

pub fn copy(text: &str) -> Copied {
    for (program, args) in COMMANDS {
        if run(program, args, text) {
            return Copied::System;
        }
    }
    Copied::Osc52(format!("\u{1b}]52;c;{}\u{7}", encode(text.as_bytes())))
}

fn run(program: &str, args: &[&str], text: &str) -> bool {
    let Ok(mut child) = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    else {
        return false;
    };
    let wrote = child
        .stdin
        .take()
        .is_some_and(|mut stdin| stdin.write_all(text.as_bytes()).is_ok());
    child.wait().is_ok_and(|status| status.success()) && wrote
}

/// Standard base64, enough for OSC 52. Small enough not to need a crate.
mod base64_lite {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    pub fn encode(bytes: &[u8]) -> String {
        let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
        for chunk in bytes.chunks(3) {
            let n = match chunk {
                [a, b, c] => (*a as u32) << 16 | (*b as u32) << 8 | *c as u32,
                [a, b] => (*a as u32) << 16 | (*b as u32) << 8,
                [a] => (*a as u32) << 16,
                _ => 0,
            };
            for i in 0..4 {
                if i <= chunk.len() {
                    out.push(TABLE[(n >> (18 - 6 * i) & 63) as usize] as char);
                } else {
                    out.push('=');
                }
            }
        }
        out
    }

    #[cfg(test)]
    mod tests {
        use super::encode;

        #[test]
        fn matches_known_vectors() {
            assert_eq!(encode(b""), "");
            assert_eq!(encode(b"f"), "Zg==");
            assert_eq!(encode(b"fo"), "Zm8=");
            assert_eq!(encode(b"foo"), "Zm9v");
            assert_eq!(encode(b"hello paddock"), "aGVsbG8gcGFkZG9jaw==");
        }
    }
}
