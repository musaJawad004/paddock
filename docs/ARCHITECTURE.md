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
src/
  main.rs           CLI entry: paddock, add, up, down, daemon
  model.rs          shared types: Project, ProcessSpec, ProcessState, Port, Stats
  config.rs         global config and paddock.toml loading
  detect/           read-only project detection
    node.rs           package.json scripts, package manager from lockfile
    rust.rs           Cargo.toml (bins, workspace members)
    compose.rs        docker-compose.yml services
    procfile.rs       Procfile and Makefile targets
  daemon/
    supervisor.rs     process lifecycle and restart policy
    pty.rs            spawn in a PTY, own process group
    logs.rs           LogBuffer: ring buffer plus vt100 screen
    ports.rs          listening ports mapped to pids and projects
    stats.rs          CPU and RAM via sysinfo
  ipc/              Request and Event enums, framing (JSON lines)
  tui/
    app.rs            state and update logic
    keys.rs           key table shared by handler and help bar
    theme.rs          colours, NO_COLOR handling
    sidebar.rs        projects, processes, ports
    logs_view.rs      live log pane, search
    palette.rs        quick jump
    status_bar.rs
```

Dependency direction: `tui` and `daemon` both depend on `model` and `ipc`.
`detect` depends only on `model`. Nothing depends on `tui`.

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
| Errors | thiserror in modules, color-eyre in main |
| Release | cargo-dist: GitHub Releases and a Homebrew tap |

Exact versions are pinned in `Cargo.toml` when the crate is scaffolded.

## Roadmap

| Version | Scope |
|---|---|
| v0.1 | Add folders, detect commands, start and stop, live logs, sidebar. Supervisor in-process. |
| v0.2 | Daemon over a Unix socket. Close and reopen without stopping servers. |
| v0.3 | Ports panel, kill strays with confirmation, open in browser, CPU and RAM. |
| v0.4 | Start all with `depends_on`, log search, crash notifications, Expo QR in the log pane. |
| v1.0 | Homebrew release, demo GIF, website. |
