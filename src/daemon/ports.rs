//! Which pid listens on which TCP port, and which Paddock process owns it.
//!
//! Every 2 s: run `lsof -nP -iTCP -sTCP:LISTEN -F pcn` (no shell), parse it
//! leniently, then walk each pid's parents (via `sysinfo`) up to a supervised
//! process group. Ports with no owner are reported as foreign, with pid and
//! command, so the TUI can offer to free them after the user confirms.
//!
//! Observes only. Paddock never opens, connects to or binds a socket here.
//! Native APIs (libproc on macOS, /proc on Linux) may replace lsof later
//! behind the same function.
