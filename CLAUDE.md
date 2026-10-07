# Paddock

A terminal workspace for your dev servers. Paddock finds your projects,
works out how to run them, starts and stops their processes, shows live logs
in one TUI, and tells you which project owns which port. Local only: no
network code, no telemetry.

Design and roadmap: `docs/ARCHITECTURE.md`. Read it before changing module
boundaries.

## Stack

Rust (edition 2024, stable), ratatui 0.30 with crossterm, tokio,
portable-pty, vt100, sysinfo, clap, serde and toml. macOS first, Linux
second.

## Commands

```bash
cargo run                                   # open the TUI (demo data for now)
cargo install --path . --locked             # install so `paddock` works anywhere
cargo run -- --no-splash --theme nord       # one-off flags; --config-path prints the config file
cargo run -- add ~/Projects/shop            # add a project; `list` and `remove <name>` too
cargo run -- --demo                         # fake data, nothing runs
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt
scripts/scan-supply-chain.py --local        # payload and network scan (also the pre-commit hook)
cargo deny check                            # advisories, licenses, banned crates
git config core.hooksPath .githooks         # once per clone
```

## Supply chain

`.github/workflows/supply-chain.yml` runs on every push, every PR and daily:

- `scan`: `scripts/scan-supply-chain.py` checks every branch for injected
  payloads and for network code in Rust outside tests. Signatures are in
  `scripts/scan-signatures.tsv`, the only file the scan skips.
- `deps`: `cargo deny check` with `deny.toml` (RustSec advisories, license
  allow list, banned network crates, crates.io only).
- `autofix`: on `main`, when `deps` fails, `scripts/autofix-deps.sh` runs
  `cargo update` and re-checks. A fix becomes a PR from `autofix/deps`;
  advisories with no compatible fix become an issue.
- `alert`: a scan finding opens an issue. Injected code is never removed
  automatically.

Pin every action to a full commit SHA with the version in a comment.

## Status

v0.1 works end to end on real projects. `paddock add <folder>` registers a
project; `detect` finds what to run; `daemon::supervisor` runs each process
in a PTY, streams its output, stops process groups (SIGTERM, SIGKILL after
5 s), kills, restarts, changes ports (`PORT` plus the tool's flag) and moves
processes between projects (saved as overrides in config.toml). Ports come
from `lsof`, CPU and memory from `sysinfo`. `paddock --demo` still shows the
fake backend in `src/demo.rs`.

The supervisor runs inside the `paddock` process for now, so quitting stops
every process (the TUI asks first). Next: v0.2 daemon over a Unix socket so
servers keep running after the TUI quits.

Tests: unit tests next to the code, `tests/supervisor.rs` and
`tests/daemon_loop.rs` run real child processes (tiny shell commands in
temp folders) and must leave no orphans.

Read a module's `//!` contract before writing code in it. If the code needs
to break the contract, change the contract and `docs/ARCHITECTURE.md` in
the same commit.

TUI render tests use insta snapshots in `src/tui/snapshots/`. After an
intended UI change, run `INSTA_UPDATE=always cargo test`, then read the
`.snap` diff before committing.

## Rules (`.claude/rules/`, loaded automatically)

- `writing-style.md`: no em or en dashes anywhere, no emoji, no AI phrasing.
  Applies to docs, comments, commit messages, help text and UI strings.
- `code-style.md`: code should read as written by a careful human. No
  narrating comments, no speculative abstractions.
- `rust.md`: error handling, async, module boundaries, dependencies.
- `process-safety.md`: process groups, only signal our own pids, no shell
  strings built from data, no network.
- `git-and-workflow.md`: the feature workflow and commit format.
- `testing-and-verification.md`: what to test and how to check for real.

## Skills (`.claude/skills/`)

Load the relevant ones before non-trivial work.

- TUI: `ratatui-tui` (written for Paddock), `testing-ratatui-tuis`.
- Rust: `rust-skills` (rule index; open only the rules you need),
  `rust-engineer`, `cli-developer`.
- Design and quality: `architecture-designer`, `code-reviewer`,
  `secure-code-guardian`.
- Workflow: `brainstorming`, `writing-plans`, `executing-plans`,
  `subagent-driven-development`, `test-driven-development`,
  `systematic-debugging`, `verification-before-completion`,
  `requesting-code-review`, `receiving-code-review`,
  `finishing-a-development-branch`, `using-git-worktrees`. Where these say
  `superpowers:<name>`, use the skill `<name>` from this folder.
- Prose: `humanizer`. Run it on README, docs and release notes.

Sources and licenses: `.claude/skills/THIRD-PARTY-LICENSES.md`.

## Agents (`.claude/agents/`)

- `rust-reviewer`: correctness, process safety, async, module boundaries.
- `style-reviewer`: anything that reads machine-made, in code or prose.

## Hooks (`.claude/settings.json`)

- `guard.sh` (before Bash): blocks `killall`, `pkill <common program>`,
  `kill -1`, force-push, `cargo publish`, reading `.env` files.
- `format.sh` (after edits): rustfmt on `.rs` files.
- `dash-check.sh` (after edits): rejects em and en dashes in our own files.
- `build-check.sh` (on stop): `cargo check` must pass once a crate exists.
- `session-context.sh` (on start): branch, last commit, open plans.
