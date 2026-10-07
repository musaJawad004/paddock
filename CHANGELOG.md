# Changelog

All notable changes to Paddock are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## Unreleased

- Paddock runs real projects. `paddock add <folder>` (or `a` in the
  dashboard) registers a project; Paddock finds what to run in
  package.json (npm, yarn, pnpm, bun, workspaces), Cargo.toml, compose
  files, a Procfile, a Makefile or paddock.toml.
- Processes run in their own terminal, so colours and QR codes survive.
  Stop sends SIGTERM to the whole process group, then SIGKILL after 5 s;
  kill is immediate.
- Ports from lsof, credited to the process that opened them; CPU and memory
  summed over each process tree.
- Changing a port sets `PORT` and the tool's own flag (Vite, Next, Expo,
  Astro and more) and restarts it. Port and project changes are saved.
- `paddock list`, `paddock remove`, `paddock --demo`.
- Quitting asks first when it would stop running processes. The splash now
  lasts three seconds.
- Splash screen with Paddy the pony and the Paddock logo. Skip with any
  key, or turn it off in settings or with `--no-splash`.
- Eight themes and a settings popup (`,`). Theme, keys and splash are saved
  to `~/.config/paddock/config.toml`.
- Every key can be rebound, in settings or in the `[keys]` table.
- Logs keep their colours and handle `\r` redraws, so Expo's QR code and
  progress lines show properly. Pick a line or a range and copy it, or copy
  all logs.
- Details view with folder, pid, uptime, port, command and resource use.
- Kill a process, change its port, move it to another project, and kill a
  listener Paddock did not start, each behind a confirmation.
- First TUI: projects, processes, live logs, ports, CPU and memory, help
  popup. Runs on a demo backend until the supervisor exists.
- Repository set up: architecture doc, contributor rules, agent skills and
  hooks.
- Supply chain workflow on every push: payload and network scan, cargo-deny,
  automatic pull requests for fixable dependency advisories.
