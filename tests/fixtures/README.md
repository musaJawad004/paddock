# Test fixtures

Small fake projects for the detectors. They are never run, only read.

| Folder | What it tests |
|---|---|
| `node-vite` | yarn from lockfile, `dev` is a process, `build` and `test` are not |
| `node-workspace` | `packageManager` field wins, pnpm workspace members become processes |
| `rust-bins` | one process per binary (`main` and `worker`) |
| `compose` | each service becomes `docker compose up <service>`, port hint 5432 |
| `procfile` | a Procfile replaces processes detected from package.json |
| `broken` | malformed package.json is skipped without an error |

`rust-bins` has an empty `[workspace]` table so cargo does not treat it as
part of Paddock's own package.
