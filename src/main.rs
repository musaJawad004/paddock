//! The `paddock` binary. Parses the command line and hands off to the
//! library: the TUI for `paddock`, the supervisor for `paddock daemon`, and
//! short one-shot commands for `add`, `remove`, `list`, `up` and `down`.
//!
//! Keeps no logic of its own. Installs `color-eyre` and file logging, then
//! calls into `paddock::cli`.

fn main() {}
