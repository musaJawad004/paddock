# Paddock architecture

Paddock is a terminal dashboard for the dev servers running on one machine.
It finds them, groups them by project, shows what they are and their logs,
and stops them when the user confirms. It never starts anything on its
own; `paddock run` only captures a command the user starts.

## Goals

- Zero setup. Servers appear because they are running, not because they
  were registered.
- Never surprise the user. Nothing is started; nothing is stopped without a
  confirmation; a shell, editor, terminal or agent is never part of a
  server.
- Local only. No network code, no telemetry, no account.
- macOS first, Linux where `lsof` exists.

## Parts

```
  paddock (TUI)  ── ipc channel ──▶  monitor (task in the same process)
   draws the screen                    ├─ ports: lsof -> (port, pid, command)
   sends confirmed actions             ├─ stats: sysinfo process table
   receives a Snapshot every 2 s       ├─ servers: group listeners into projects and servers
   receives Logs every 250 ms          ├─ follow: tail the selected server's log file
                                       └─ actions: re-check, then signal

  paddock run <cmd>  ── exec ──▶  script -q -F <log> <cmd>   (in the user's terminal)
```

The TUI and the monitor only exchange `ipc` messages, so the monitor could
later move into a background process without touching the TUI.

## Source layout

```
Cargo.toml            dependencies, lints, release profile
deny.toml             dependency policy (advisories, licenses, banned crates)
src/
  main.rs             binary entry: error reporting, then cli
  lib.rs              module list and the dependency rules below
  cli.rs              paddock, paddock list, --demo, --theme, --no-splash
  model.rs            Snapshot, Project, Server, ServerId, ListeningPort
  config.rs           ~/.config/paddock/config.toml (theme, keys, splash)
  project.rs          which folder is a project, from its files
  capture.rs          paddock run (script wrapper), paddock init, log folder
  demo.rs             made-up servers for --demo
  monitor/
    mod.rs            scan loop, request handling, SIGKILL after 5 s
    ports.rs          lsof field output parser
    stats.rs          sysinfo process table (parents, cwd, cmd, owner, usage)
    servers.rs        listeners + process table -> projects and servers (pure)
    follow.rs         reads a growing log file from where it left off
    logs.rs           bytes to lines (split chunks, UTF-8, prompts)
    actions.rs        the only code that sends signals
  ipc/                Request and Event, in-process channel transport
  tui/
    mod.rs            event loop; copy, open and save run off the UI thread
    app.rs            state, update, effects, layout
    keys.rs           actions, default keys, config overrides, rebinding
    theme.rs          8 palettes, 256-colour fallback, NO_COLOR
    sidebar.rs        projects and servers, ports
    details.rs        the selected server's header, empty state
    logs_view.rs      log pane: colours, cursor, selection, scrollbar
    ansi.rs           ANSI colours and \r redraws in log lines
    overlay.rs        help, settings and confirmation popups
    settings.rs       theme, keys and splash settings, saved at once
    help.rs           the ? popup, built from the keymap
    splash.rs         start-up animation
    brand.rs          logo and Paddy the pony
    clipboard.rs      pbcopy, wl-copy, xclip, xsel, OSC 52 fallback
    status_bar.rs     header counts, footer hints, notices
tests/
  monitor.rs          real processes: found, stopped, captured with paddock run
```

Dependency rules:

```
cli ──▶ tui ─────┐   (tui also reads and writes config)
  ├───▶ demo ────┤
  ├───▶ monitor ─┼──▶ ipc ──▶ model
  │      ├──▶ project
  └───▶ capture ◀┘   (monitor only reads its logs folder)
```

`tui` and `monitor` never import each other.

## Finding servers

Every scan:

1. `lsof -nP -iTCP -sTCP:LISTEN -F pcn` lists listeners.
2. A listener is a dev server when its process belongs to the user and its
   working folder is below the home folder.
3. Its project is the outermost folder with a project file around that
   working folder (`project::root`), or the folder itself.
4. The server is found by walking up the parents while they run in that
   project and are dev runners: package managers, `sh -c` and script
   shells, interpreters (`node`, `python`...) unless they run a coding
   agent, cargo, make and similar. An interactive shell, an editor, a
   terminal or an agent ends the walk. The highest process reached is the
   top of the server; its pid and start time are the `ServerId`.
5. Listeners of the same tree merge into one server with several ports.
6. Everything else (system apps, other users, anything outside home) is
   not shown.
7. If the server's top process has a `script` parent whose arguments name
   a file in the logs folder, that file is the server's log.

`servers::collect` is pure: it reads everything through the `ProcessTable`
trait, so the rules above are unit tested with made-up tables.

## Stopping

- Stop: SIGTERM to every process in the tree, children first. If the server
  is still there five seconds later, SIGKILL.
- Kill: SIGKILL to every process in the tree.

Before any signal, `actions` refreshes the process table and checks that the
pid still has the start time the user saw, so a reused pid is never hit.
Paddock itself and every process above it are never signalled, nor is
pid 1.

## Logs

A running process's output belongs to the terminal that started it, so
Paddock captures logs at start instead of reading them later:

- `paddock run <cmd>` execs `script -q -F <log> <cmd>` (macOS; util-linux
  `script -q -f -e -c` on Linux), keeping its pid. The user's terminal sees
  everything as before; the log gets a copy, flushed on every write. The
  exit code is the command's.
- It captures only when stdin and stdout are a terminal, and with `--auto`
  only commands that start a server. Anything else is exec'd untouched.
- `paddock init zsh|bash|fish` prints functions for npm, pnpm, yarn, bun,
  npx, pnpx, bunx, cargo and deno that call `paddock run --auto`.
- Logs go to `$XDG_STATE_HOME/paddock/logs` (default
  `~/.local/state/paddock/logs`, mode 700) and are deleted after 3 days.
- The monitor follows the selected server: on `Follow` it reads the last
  256 KB, then every 250 ms only what was appended.

## Configuration

```toml
[ui]
theme = "paddock"
splash = true

[keys]
stop = "x"
```

## Stack

| Concern | Choice |
|---|---|
| TUI | ratatui 0.30 (crossterm backend) |
| Async | tokio |
| Process table, CPU, memory | sysinfo |
| Ports | `lsof` field output |
| Signals | nix |
| Config | serde, toml |
| CLI | clap (derive) |
| Errors | thiserror in modules, color-eyre in main |

Every dependency must pass `deny.toml`.

## Decisions

1. **Watch, never start.** An earlier version started projects itself and
   ended up running a second copy of a server that was already up on
   another port. Starting is the user's job, in their own terminal.
2. **Logs are captured at start, not read later.** A server's output belongs
   to the terminal that started it; macOS gives no other program a way to
   read it without root tracing. `script` sits between the command and the
   terminal instead, which the shell line makes automatic.
3. **A server is a process tree, bounded by runners.** Walking up through
   known dev runners only (and never into agents, editors or interactive
   shells) makes "stop this server" mean what the user expects.
4. **`ServerId` is pid plus start time.** Pids are reused; the pair is not.
5. **`lsof` for ports.** It is on every Mac and gives pid and port in one
   call. Native APIs can replace it behind `monitor::ports`.
6. **No network code, enforced.** `deny.toml` bans HTTP and TLS crates; the
   supply chain scan rejects TCP and UDP APIs in `src/`.
7. **`unsafe_code = "deny"`.** Nothing here needs it.

## Roadmap

| Version | Scope |
|---|---|
| v0.1 | Find, group and show running servers; live logs through paddock run and paddock init; open, copy, stop, kill; themes, keys, splash. |
| v0.2 | Port history (what was on :3000 earlier), notifications when a server stops on its own. |
| v1.0 | Homebrew release, demo GIF, website. |
