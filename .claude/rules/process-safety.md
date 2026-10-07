---
paths:
  - "src/daemon/**"
  - "src/detect/**"
---
# Process safety

Paddock starts and stops other programs on the user's machine. Mistakes here
kill someone's unsaved work.

- Every child is started in its own process group (`setsid`/`process_group(0)`)
  so stop and restart reach the whole tree (`yarn` → `node` → `esbuild`).
- Stop is graceful: SIGTERM to the group, wait (default 5 s), then SIGKILL.
- Only signal processes Paddock started, tracked by pid **and** start time
  (pids get reused). The ports view may offer to kill a foreign process, but
  only after the user confirms in the UI, naming the pid and command.
- Commands come from the user's own project files or `paddock.toml`. Run
  them through the user's shell (`$SHELL -lc`) so their PATH and version
  managers work, but never build a command string out of file names or
  other data Paddock read; pass those as arguments.
- Detection is read-only. Paddock never writes into a project folder except
  `paddock.toml`, and only when the user asks.
- No network code. Paddock does not phone home, check for updates or send
  telemetry. "Open in browser" hands the URL to the OS and stops there.
- Never log environment variables or the contents of `.env` files.
