# Paddock

A terminal workspace for your dev servers.

Every project, every process and every port in one place. Paddock finds your
projects, works out how to run them, and lets you start, stop and watch them
from a single TUI. Close it and your servers keep running. Open it again and
they are still there.

> Early development. The dashboard runs on demo data for now: nothing is
> started on your machine yet. The plan is in
> [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Try it

Needs Rust 1.95 or newer.

```bash
cargo install --git https://github.com/musaJawad004/paddock --locked
paddock
```

| Key | Does |
|---|---|
| `↑` `↓` | pick a process (or a port, or a log line) |
| `tab` | move between processes, ports and logs |
| `s` `x` `r` | start, stop, restart |
| `K` | kill now, or kill whatever holds the selected port |
| `p` / `m` | change the port / move to another project |
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

## What it will do

- Detect how to run a project from `package.json`, `Cargo.toml`,
  `docker-compose.yml` or a `Procfile`. No config needed for common setups.
- Start everything a project needs with one key, in the right order.
- Show live logs per process, with colours, search and a red marker when
  something crashes.
- List every listening port, which project owns it, and let you free a port
  held by a stray process.
- Open a project in the browser, or show the Expo QR code for your phone.

Local only. Paddock has no network code and sends no telemetry.

## License

MIT. See [LICENSE](LICENSE).
