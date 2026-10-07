# Testing and verification

- `cargo check` must pass before a turn ends (Stop hook). `cargo test` and
  `cargo clippy --all-targets -- -D warnings` before committing.
- Unit tests for: each detector (fixture project folders in a tempdir),
  supervisor state transitions, log ring buffer, port parsing (saved `lsof`
  output as fixtures), TUI update logic.
- TUI rendering: `TestBackend` snapshots with fixed sizes and fixture data.
- Process tests spawn tiny known commands (`sh -c 'echo hi; sleep 5'`), never
  real dev servers, and always clean up their process group.
- Real check before saying a feature works: `cargo run`, add a sample project
  (`tests/fixtures/`), start and stop it, quit, and confirm no orphan
  processes are left (`pgrep -fl <fixture name>`).
- Never test against the user's real projects or kill processes the test did
  not start.
