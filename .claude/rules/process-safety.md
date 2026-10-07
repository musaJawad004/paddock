---
paths:
  - "src/monitor/**"
  - "src/tui/mod.rs"
---
# Process safety

Paddock watches the user's processes and stops them when asked. A mistake
here kills someone's unsaved work, their shell, or their editor.

- Paddock never starts a process on its own. The dashboard runs only `lsof`
  (to read ports) and, on user request, `open`/`xdg-open` and
  `pbcopy`/`wl-copy`/`xclip`/`xsel`/`clip`, each with fixed arguments and no
  shell.
- Paddock never wraps, intercepts or changes how the user's commands run:
  no shell functions, no aliases, no PATH changes, no edits to shell config
  files.
- Every signal is sent from `monitor::actions` and nowhere else, only for a
  target the user confirmed in the TUI.
- Before signalling, refresh the process table and check the pid still has
  the start time the user saw. A reused pid is never signalled.
- Never signal pid 1, Paddock itself, or any process above it (its shell and
  terminal).
- A server's tree is bounded by dev runners (`monitor::servers`): never
  include interactive shells, editors, terminals or coding agents. When in
  doubt, stop the walk lower; a server that stops too little is better than
  one that takes the user's shell with it.
- Stop is graceful: SIGTERM to the tree, children first, SIGKILL after 5 s.
- No network code. Opening a URL hands it to the OS and stops there.
- Never log or display environment variables (`clean_cmd` strips ones that
  leak into command lines).
