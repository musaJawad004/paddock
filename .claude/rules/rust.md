---
paths:
  - "src/**/*.rs"
  - "tests/**/*.rs"
  - "Cargo.toml"
---
# Rust conventions

- Edition 2024, stable toolchain. `cargo fmt` and
  `cargo clippy --all-targets -- -D warnings` clean.
- No `unwrap()`/`expect()` outside tests and `main` setup. Libraries return
  `thiserror` enums; the binary uses `color-eyre` for reports.
- Never panic on input we do not control: `lsof` output, the process table
  (processes vanish between two reads), config.toml. Parse leniently and
  skip what you do not understand.
- Async runtime is tokio. Blocking work (file scans, `lsof`) goes through
  `spawn_blocking` or a dedicated thread, never inside the TUI loop.
- Channels between parts (`tokio::sync::mpsc`), not shared mutable state.
  If a mutex is unavoidable, hold it for microseconds and never across an
  `.await`.
- Modules talk through the types in `src/model.rs` and the messages in
  `src/ipc/`. `monitor` and `tui` do not import each other.
- Platform code sits behind `#[cfg(unix)]` / `#[cfg(target_os = "...")]` in
  the module that owns the behaviour. macOS and Linux must both compile.
- Dependencies: add one only when it saves real work. Check it is
  maintained, check its license (MIT/Apache-2.0/BSD), and pin the minor
  version.
- Tests sit next to the code in `#[cfg(test)]`, using fixtures in
  `tempfile::tempdir()`. Never read the user's real projects in tests.
- Load the `rust-skills` and `rust-engineer` skills before non-trivial Rust
  work.
