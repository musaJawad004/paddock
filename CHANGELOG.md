# Changelog

All notable changes to Paddock are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## Unreleased

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
