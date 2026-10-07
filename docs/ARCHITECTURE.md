# Paddock architecture

Paddock is a terminal workspace for your dev servers. It finds your
projects, works out how to run them, starts and stops their processes, shows
live logs, and tells you which project owns which port.

## Goals

- Zero config for common projects. Detection first, `paddock.toml` only when
  detection is not enough.
- Servers keep running after you close the dashboard, like a terminal
  multiplexer. Reopen Paddock and everything is still there.
- Local only. No network code, no telemetry, no account.
- macOS first, Linux second. Windows is out of scope until v1.

## Processes and parts

```
  paddock (TUI)  ── Unix socket ──▶  paddock daemon (background)
   draws the screen                    ├─ Supervisor: start, stop, restart, crash detection
   sends user actions                  ├─ One PTY per child process
   receives events                     ├─ LogBuffer per process (bounded ring)
                                       ├─ Ports: which pid owns which port
                                       ├─ Stats: CPU and RAM per project
                                       └─ Detect: reads projects, finds run commands
```

- `paddock` with no arguments opens the TUI. If no daemon is running, it
  starts one (same binary, `paddock daemon`) and connects to it.
- The daemon owns every child process. The TUI holds no process handles, so
  quitting or crashing the TUI never kills a server.
- `paddock down` stops all processes and the daemon.

v0.1 runs the supervisor inside the TUI process (no daemon) to get a working
tool quickly. The TUI already talks to the supervisor only through `ipc`
messages over a channel, so v0.2 swaps the channel for a Unix socket without
touching the TUI.

## Source layout

```
Cargo.toml            dependencies, lints, release profile
deny.toml             dependency policy (advisories, licenses, banned crates)
src/
  main.rs             binary entry: error reporting, logging, then cli
  lib.rs              module list and the dependency rules below
  cli.rs              clap commands: paddock, add, remove, list, up, down, daemon
  model.rs            shared types: Project, ProcessSpec, ProcessState, ListeningPort
  config.rs           global config and paddock.toml
  detect/             read-only project detection
    mod.rs              runs every detector and merges results
    node.rs             package.json, package manager, workspaces
    rust.rs             Cargo.toml binaries and workspace members
    compose.rs          compose services
    procfile.rs         Procfile and Makefile targets
  daemon/             owns child processes (in-process in v0.1)
    mod.rs
    supervisor.rs       lifecycle state machine, restart policy
    pty.rs              spawn in a PTY, own process group
    logs.rs             vt100 screen plus bounded scrollback
    ports.rs            listening ports to pids to processes
    stats.rs            CPU and memory
  ipc/                the only link between tui and daemon
    mod.rs
    protocol.rs         Request and Event, JSON lines, PROTOCOL_VERSION
    transport.rs        channel in v0.1, Unix socket in v0.2
  tui/
    mod.rs
    app.rs              state, update, event loop
    keys.rs             key table shared by handler and help
    theme.rs            colours, NO_COLOR
    sidebar.rs          projects, processes, ports
    logs_view.rs        log pane, search, attach
    palette.rs          quick jump and rare commands
    status_bar.rs       key hints, connection state, errors
tests/
  fixtures/           fake projects for the detectors (read, never run)
```

Every module starts with a `//!` contract: what it owns, what it must not
do. Read it before changing the module.

Dependency rules:

```
cli ──▶ tui ─────┐
  └───▶ daemon ──┼──▶ ipc ──▶ model
         └──▶ detect ───────▶ model
config ─────────────────────▶ model
```

`tui` and `daemon` never import each other. `detect` never runs anything.

## Data flow

1. Startup: load `~/.config/paddock/config.toml` (list of project folders).
   For each folder, merge `paddock.toml` (if present) over detected
   commands to get `Vec<ProcessSpec>`.
2. User presses `s` on a process: TUI sends `Request::Start { id }`.
3. Supervisor spawns the command via `$SHELL -lc` in a PTY, in its own process
   group, and emits `Event::State { id, Starting }`.
4. PTY output is read on a task, appended to the `LogBuffer`, and forwarded as
   `Event::Output { id, bytes }`, batched to at most 30 events a second.
5. The port scanner runs every 2 s, maps listening ports to pids, walks the
   process tree up to a supervised child, and emits `Event::Ports(...)`.
6. On exit, the supervisor records the code. Non-zero becomes `Crashed`, and
   a restart with backoff (1 s, 2 s, 4 s, max 30 s) follows if the spec has
   `restart = "on-failure"`.

## Process states

```
Stopped ─start─▶ Starting ─output or port seen─▶ Running
Running ─exit 0─▶ Exited
Running ─exit ≠ 0 or signal─▶ Crashed ─(restart policy)─▶ Starting
any ─stop─▶ Stopping ─group gone─▶ Stopped
```

Stop sends SIGTERM to the process group, waits 5 s, then SIGKILL.

## Configuration

Global, `~/.config/paddock/config.toml`:

```toml
projects = ["~/Projects/lumo", "~/Projects/heron"]
scrollback = 10000
```

Per project, `paddock.toml` (optional):

```toml
[process.app]
cmd = "yarn expo start"
port = 8081

[process.api]
cmd = "yarn dev"
cwd = "server"
depends_on = ["db"]
restart = "on-failure"

[process.db]
cmd = "docker compose up postgres"
```

## Stack

| Concern | Choice |
|---|---|
| TUI | ratatui 0.30 (crossterm backend, re-exported) |
| Async | tokio |
| Child processes | portable-pty |
| Terminal output | vt100 |
| CPU and RAM | sysinfo |
| Ports | parse `lsof -nP -iTCP -sTCP:LISTEN` first, native APIs later |
| Config | serde, toml |
| CLI | clap (derive) |
| Signals | nix (same version as portable-pty) |
| Logging | tracing, written to a file because the TUI owns the terminal |
| Errors | thiserror in modules, color-eyre in main |
| Release | cargo-dist: GitHub Releases and a Homebrew tap |

Exact versions are in `Cargo.toml`. Every dependency must pass `deny.toml`.

## Decisions

Short records of choices that are expensive to reverse.

1. **Library plus thin binary.** All logic lives in `src/lib.rs` modules so
   integration tests in `tests/` can use it. `main.rs` only wires errors,
   logging and the CLI.
2. **Supervisor in-process first, daemon second.** v0.1 ships faster with
   one process. Because `tui` and `daemon` already talk only through `ipc`,
   v0.2 changes the transport, not the TUI.
3. **PTY per child (portable-pty), not plain pipes.** Dev servers detect a
   terminal and change behaviour without one: no colours, no QR code,
   different buffering. A PTY also puts the child in its own session, which
   gives us the process group we need to stop the whole tree.
4. **`lsof` for ports in v0.1.** It is on every Mac and most Linux machines
   and gives pid and port in one call. Native APIs can replace it later
   behind `daemon::ports` without changing callers.
5. **No `dirs` crate.** Config goes to `$XDG_CONFIG_HOME/paddock` or
   `~/.config/paddock` on both platforms. This also kept an MPL-2.0
   dependency out of the tree.
6. **No network code, enforced.** `deny.toml` bans HTTP and TLS crates; the
   supply chain scan rejects TCP and UDP APIs in `src/`. The daemon in v0.2
   uses a Unix socket only.
7. **`unsafe_code = "deny"`.** Process-group setup is done by portable-pty;
   Paddock itself should not need unsafe. An exception needs a comment
   explaining why and a review.

## Roadmap

| Version | Scope |
|---|---|
| v0.1 | Add folders, detect commands, start and stop, live logs, sidebar. Supervisor in-process. |
| v0.2 | Daemon over a Unix socket. Close and reopen without stopping servers. |
| v0.3 | Ports panel, kill strays with confirmation, open in browser, CPU and RAM. |
| v0.4 | Start all with `depends_on`, log search, crash notifications, Expo QR in the log pane. |
| v1.0 | Homebrew release, demo GIF, website. |
