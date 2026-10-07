//! Which pid listens on which TCP port.
//!
//! Runs `lsof -nP -iTCP -sTCP:LISTEN -F pcn` (no shell, fixed arguments) and
//! parses its field output. Matching listeners to supervised processes is
//! the supervisor's job, using the process tree from `stats`.
//!
//! Observes only: Paddock never opens, connects to or binds a socket here.
//! Native APIs (libproc on macOS, /proc on Linux) may replace lsof later
//! behind the same function.

use std::io;
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listener {
    pub port: u16,
    pub pid: u32,
    pub command: String,
}

pub fn scan() -> Result<Vec<Listener>, String> {
    let output = Command::new("lsof")
        .args(["-nP", "-iTCP", "-sTCP:LISTEN", "-F", "pcn"])
        .output()
        .map_err(|err| match err.kind() {
            io::ErrorKind::NotFound => "lsof is not installed, so ports cannot be shown".to_owned(),
            _ => format!("cannot run lsof: {err}"),
        })?;
    // lsof exits with 1 when nothing matches; that is an empty list.
    Ok(parse(&String::from_utf8_lossy(&output.stdout)))
}

/// Field output: `p<pid>` starts a process, `c<command>` names it, then one
/// `f<fd>` and `n<address>` per socket. Addresses look like `*:3000`,
/// `127.0.0.1:5432` or `[::1]:8081`.
pub fn parse(text: &str) -> Vec<Listener> {
    let mut out: Vec<Listener> = Vec::new();
    let mut pid = None;
    let mut command = String::new();
    for line in text.lines() {
        let (field, value) = line.split_at(line.len().min(1));
        match field {
            "p" => {
                pid = value.parse().ok();
                command.clear();
            }
            "c" => command = value.to_owned(),
            "n" => {
                let port = value.rsplit(':').next().and_then(|p| p.parse().ok());
                if let (Some(pid), Some(port)) = (pid, port) {
                    let listener = Listener {
                        port,
                        pid,
                        command: command.clone(),
                    };
                    if !out.contains(&listener) {
                        out.push(listener);
                    }
                }
            }
            _ => {}
        }
    }
    out.sort_by_key(|l| (l.port, l.pid));
    out
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn parses_field_output_and_merges_ipv4_and_ipv6() {
        let text = "p812\ncControlCenter\nf9\nn*:5000\nf10\nn*:7000\np41000\ncnode\nf23\nn127.0.0.1:3000\nf24\nn[::1]:3000\np9\ncbroken\nnnot-an-address\n";
        assert_eq!(
            parse(text),
            vec![
                Listener {
                    port: 3000,
                    pid: 41000,
                    command: "node".into()
                },
                Listener {
                    port: 5000,
                    pid: 812,
                    command: "ControlCenter".into()
                },
                Listener {
                    port: 7000,
                    pid: 812,
                    command: "ControlCenter".into()
                },
            ]
        );
    }

    #[test]
    fn empty_output_is_no_listeners() {
        assert!(parse("").is_empty());
    }
}
