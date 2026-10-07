# Paddock

A terminal workspace for your dev servers.

Every project, every process and every port in one place. Paddock finds your
projects, works out how to run them, and lets you start, stop and watch them
from a single TUI. Close it and your servers keep running. Open it again and
they are still there.

> Early development. Nothing to install yet. The plan is in
> [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

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
