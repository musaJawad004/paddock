# Changelog

All notable changes to Paddock are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## Unreleased

## 0.1.0 (2026-10-07)

- Runs on macOS, Linux and Windows. On Windows, ports come from netstat and
  stopping a server ends it at once; log capture needs `script`, so it is
  macOS and Linux only for now.
- Install with Homebrew (`brew tap musaJawad004/paddock
  https://github.com/musaJawad004/paddock`), npm (`npm install -g
  paddock-cli`), cargo, or a release archive. Releases are built by CI for
  macOS (Apple silicon and Intel), Linux (x64 and arm64) and Windows.
- Paddock watches instead of starting. It finds every dev server running
  on the machine (from lsof and the process table), groups them by project,
  and shows ports, command, folder, uptime, CPU and memory. Nothing is
  registered or started; servers appear because they are running.
- A server is the whole tree the user started (`yarn dev` and everything
  under it). Shells, editors, terminals and coding agents are never part of
  a server, so stopping a server never stops them.
- Actions: open in the browser, copy URL or command, stop (SIGTERM, then
  SIGKILL after 5 s), kill, free a port held by another program. Each is
  confirmed, and every pid is re-checked by start time before a signal.
- Live logs: `paddock run <cmd>` runs a command through the system's
  `script` tool, so the terminal works as before and a copy of the output
  goes to a private log file. `eval "$(paddock init zsh)"` in ~/.zshrc does
  this automatically for dev server commands (npm run dev, yarn dev, npx
  expo start, cargo run...). The log pane shows colours, follows new lines,
  and copies a line, a selection or everything.
- System apps and other users' programs are no longer listed.
- `paddock list` prints the running servers once.
- Removed: adding projects, starting and restarting processes from the
  dashboard, changing ports, moving processes, freeing ports held by other
  programs, and the folder picker.
- Splash with Paddy the pony, eight themes, settings popup, rebindable
  keys.
- Repository set up: architecture doc, contributor rules, agent skills,
  hooks, CI and a supply chain workflow.
