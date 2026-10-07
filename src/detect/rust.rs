//! Rust projects: `Cargo.toml`.
//!
//! A package with one binary becomes `cargo run`. Several binaries become one
//! process each (`cargo run --bin <name>`). Workspaces list members that have
//! binaries. Library-only crates produce nothing.
//!
//! Uses `cargo watch` or `bacon` only if `paddock.toml` asks for it; Paddock
//! does not guess which watcher the user has installed.
