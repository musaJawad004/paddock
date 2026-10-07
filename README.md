# Paddock

Every dev server on this machine, in one place.

Paddock is a terminal dashboard. It finds the dev servers you started in any
terminal, editor or agent, groups them by project, and shows what each one
is: its ports, command, folder, uptime, CPU and memory. From there you can
open one in the browser, copy its URL or command, stop it, or free a port
something else is holding.

Paddock never starts anything. It watches, and it stops only what you
confirm.

> Early development (v0.1). macOS first; Linux works where `lsof` is
> installed. The design is in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Try it

Needs Rust 1.95 or newer.

```bash
cargo install --git https://github.com/musaJawad004/paddock --locked

paddock            # the dashboard
paddock list       # the same, printed once
paddock --demo     # look around with made-up servers
```

| Key | Does |
|---|---|
| `↑` `↓` | pick a server (or a port) |
| `tab` | switch between servers and ports |
| `o` | open it in the browser |
| `y` / `Y` | copy its URL / its full command |
| `x` | stop it: SIGTERM to its whole process tree, SIGKILL after 5 s |
| `K` | kill it now, or free the selected port |
| `,` | settings: theme, keys, splash |
| `?` / `q` | help / quit |

Every key can be changed in settings or in `~/.config/paddock/config.toml`:

```toml
[ui]
theme = "paddock"   # terminal, catppuccin-mocha, dracula, nord, gruvbox, tokyo-night, solarized-light
splash = true

[keys]
stop = "s"
quit = ["q", "ctrl+q"]
```

## How it finds servers

Every two seconds Paddock asks `lsof` which programs listen on which ports,
and the process table who started them. A listener counts as a dev server
when it belongs to you and runs in a folder below your home folder. Its
project is the outermost folder around it with a project file
(`package.json`, `Cargo.toml`, `go.mod`, `pyproject.toml`...).

The server is the whole tree you started: `yarn dev` and the `sh`, `vite`
and `esbuild` processes under it count as one server called `dev`. Paddock
never counts your shell, editor, terminal or a coding agent such as Claude
Code as part of a server, so stopping a server never stops them.

## What it cannot do

A server's output goes only to the terminal that started it, and no other
program can read it. So Paddock shows everything about a server except its
logs.

## Privacy

Paddock reads the process table and runs `lsof`. It has no network code and
sends no telemetry; the build fails if network code is added.

## License

MIT. See [LICENSE](LICENSE).
