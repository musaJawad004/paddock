# Paddock

A terminal workspace for your dev servers.

Every project, every process and every port in one place. Paddock finds your
projects, works out how to run them, and lets you start, stop and watch them
from a single TUI. Close it and your servers keep running. Open it again and
they are still there.

> Early development (v0.1). Paddock runs your real projects, but it still
> lives inside the dashboard: quitting stops your servers (it asks first).
> Keeping them running in the background comes next. The plan is in
> [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Try it

Needs Rust 1.95 or newer.

```bash
cargo install --git https://github.com/musaJawad004/paddock --locked

cd ~/Projects/my-app && paddock add    # add a project (or press a inside)
paddock                                # open the dashboard
paddock list                           # what Paddock will run, without the UI
paddock --demo                         # look around with made-up projects
```

| Key | Does |
|---|---|
| `↑` `↓` | pick a process (or a port, or a log line) |
| `tab` | move between processes, ports and logs |
| `s` `x` `r` | start, stop, restart |
| `K` | kill now, or kill whatever holds the selected port |
| `p` / `m` | change the port / move to another project |
| `a` / `D` | add a project (folder picker, or the selected running server) / remove one |
| `i` | details: folder, pid, uptime, command, CPU, memory |
| `v` `y` `Y` | select lines, copy them, copy all logs |
| `,` | settings: theme, keys, splash |
| `?` / `q` | help / quit |

Every key can be changed in settings or in `~/.config/paddock/config.toml`:

```toml
[ui]
theme = "paddock"   # terminal, catppuccin-mocha, dracula, nord, gruvbox, tokyo-night, solarized-light
splash = true

[keys]
start = "S"
quit = ["q", "ctrl+q"]
```

## What it does

- Find dev servers you already started in other terminals and offer to add
  their projects.
- Detect how to run a project from `package.json`, `Cargo.toml`,
  `docker-compose.yml` or a `Procfile`. No config needed for common setups.
- Start, stop, restart or kill each process; stopping reaches the whole
  process tree, so nothing is left holding a port.
- Show live logs per process, with colours, search and a red marker when
  something crashes.
- List every listening port, which project owns it, and let you free a port
  held by a stray process.
- Show Expo's QR code and every tool's own colours in the log pane.
- Real CPU and memory per process tree, from your machine.

Local only. Paddock has no network code and sends no telemetry.

## License

MIT. See [LICENSE](LICENSE).
