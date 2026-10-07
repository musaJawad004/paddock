//! Which pid listens on which TCP port.
//!
//! Runs `lsof -nP -iTCP -sTCP:LISTEN -F pcn` (no shell, fixed arguments) and
//! parses its field output. On Windows, `netstat -ano` instead; it does not
//! name the program, so the command is filled in from the process table. Matching listeners to servers is the job of
//! `servers`, using the process tree from `stats`.
//!
//! Observes only: Paddock never opens, connects to or binds a socket here.
//! Native APIs (libproc on macOS, /proc on Linux) may replace lsof later
//! behind the same function.

#[cfg(not(windows))]
use std::io;
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listener {
    pub port: u16,
    pub pid: u32,
    pub command: String,
}

#[cfg(not(windows))]
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

#[cfg(windows)]
pub fn scan() -> Result<Vec<Listener>, String> {
    let output = Command::new("netstat")
        .args(["-ano", "-p", "TCP"])
        .output()
        .map_err(|err| format!("cannot run netstat: {err}"))?;
    let mut listeners = parse_netstat(&String::from_utf8_lossy(&output.stdout));
    let v6 = Command::new("netstat")
        .args(["-ano", "-p", "TCPv6"])
        .output();
    if let Ok(v6) = v6 {
        listeners.extend(parse_netstat(&String::from_utf8_lossy(&v6.stdout)));
    }
    listeners.sort_by_key(|l| (l.port, l.pid));
    listeners.dedup();
    Ok(listeners)
}

/// `  TCP    0.0.0.0:3000    0.0.0.0:0    LISTENING    1234` and the
/// `[::]:3000` form. The state word is localised on some Windows versions,
/// so a listening socket is recognised by its remote port being 0.
pub fn parse_netstat(text: &str) -> Vec<Listener> {
    text.lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            let [proto, local, remote, .., pid] = fields.as_slice() else {
                return None;
            };
            if !proto.eq_ignore_ascii_case("tcp") || !remote.ends_with(":0") {
                return None;
            }
            Some(Listener {
                port: local.rsplit(':').next()?.parse().ok()?,
                pid: pid.parse().ok()?,
                command: String::new(),
            })
        })
        .collect()
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
    fn parses_windows_netstat() {
        let text = "\nActive Connections\n\n  Proto  Local Address  Foreign Address  State  PID\n  TCP    0.0.0.0:3000   0.0.0.0:0   LISTENING   1234\n  TCP    [::]:5173   [::]:0   LISTENING   88\n  TCP    127.0.0.1:50000   127.0.0.1:3000   ESTABLISHED   1234\n";
        assert_eq!(
            parse_netstat(text),
            vec![
                Listener {
                    port: 3000,
                    pid: 1234,
                    command: String::new()
                },
                Listener {
                    port: 5173,
                    pid: 88,
                    command: String::new()
                },
            ]
        );
    }

    #[test]
    fn empty_output_is_no_listeners() {
        assert!(parse("").is_empty());
    }
}
