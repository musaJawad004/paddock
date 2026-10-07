# Testing and verification

- `cargo check` must pass before a turn ends (Stop hook). `cargo test` and
  `cargo clippy --all-targets -- -D warnings` before committing.
- Unit tests for: server grouping (`monitor::servers` with made-up process
  tables), port parsing (saved `lsof` output), project roots (temp
  folders), TUI update logic.
- Tests that open real sockets (port detection) go in `tests/`, not in an
  inline `#[cfg(test)]` module. The supply chain scan rejects network APIs in
  `src/`, including inline tests.
- TUI rendering: `TestBackend` snapshots with fixed sizes and fixture data.
- Process tests spawn tiny known commands (small Python listeners), never
  real dev servers, and clean up with a guard that kills on drop.
- Real check before saying a feature works: start a small server in a temp
  project folder from a terminal, run `cargo run -- list` and the dashboard,
  stop it from the TUI, and confirm nothing is left (`pgrep -fl <name>`).
  `paddock list` against the user's real machine is read-only and fine.
- Never test against the user's real projects or kill processes the test did
  not start.
