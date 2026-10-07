# Paddock

Every dev server on this machine, in one place.

Paddock is a terminal dashboard. It finds the dev servers you started in any
terminal, editor or agent, groups them by project, and shows what each one
is: its ports, command, folder, uptime, CPU, memory and live logs. From
there you can open one in the browser, copy its URL, command or log lines,
and stop it.

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

To see logs, add this line to `~/.zshrc` once (bash and fish work too):

```bash
eval "$(paddock init zsh)"
```

From then on, `npm run dev`, `yarn dev`, `npx expo start`, `cargo run` and
other dev server commands started in any terminal show their logs in
Paddock. Your terminal shows the output exactly as before. Other commands
(`npm install`, `cargo build`) run untouched. To capture a single run
without the shell line, use `paddock run npm run dev`.

| Key | Does |
|---|---|
| `↑` `↓` | pick a server, a log line or a port |
| `tab` | switch between servers, logs and ports |
| `o` | open it in the browser |
| `y` / `Y` | servers: copy URL / command. Logs: copy line or selection / all |
| `v` | start a selection in the logs |
| `x` | stop it: SIGTERM to its whole process tree, SIGKILL after 5 s |
| `K` | kill it now |
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

## How logs work

A server's output goes only to the terminal that started it; no other
program can read it afterwards. So logs come from the start instead:
`paddock run` (which the shell line calls for you) runs the command through
the system's `script` tool, which shows everything in your terminal as
usual and writes a copy to `~/.local/state/paddock/logs`. That folder is
readable only by you, and logs older than three days are deleted.

A server started before you added the shell line shows everything except
logs, plus the exact `paddock run` command to restart it with them.

## Privacy

Paddock reads the process table, runs `lsof`, and reads its own log files.
It has no network code and sends no telemetry; the build fails if network
code is added.

## License

MIT. See [LICENSE](LICENSE).
