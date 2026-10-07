---
paths:
  - "src/monitor/**"
  - "src/tui/mod.rs"
---
# Process safety

Paddock watches the user's processes and stops them when asked. A mistake
here kills someone's unsaved work, their shell, or their editor.

- Paddock never starts a process. The only programs it runs are `lsof` (to
  read ports) and the OS's `open`/`xdg-open`, `pbcopy`/`wl-copy`/`xclip`/
  `xsel` (on user request), each with fixed arguments and no shell.
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
- A foreign listener is signalled only after a fresh `lsof` shows it still
  holds the port.
- No network code. Opening a URL hands it to the OS and stops there.
- Never log or display environment variables (`clean_cmd` strips ones that
  leak into command lines).
