//! Node projects: `package.json` scripts.
//!
//! - Package manager from the lockfile: `yarn.lock` (yarn), `pnpm-lock.yaml`
//!   (pnpm), `bun.lock`/`bun.lockb` (bun), otherwise npm. `packageManager`
//!   in package.json wins when present.
//! - Long-running scripts become processes: `dev`, `start`, `serve`,
//!   `watch`, and anything whose command runs a known dev server (vite, next
//!   dev, expo start, nodemon, tsx watch). Build and test scripts are not
//!   processes.
//! - Expected ports from well-known defaults (vite 5173, next 3000, expo
//!   8081), only as a hint; the real port comes from `daemon::ports`.
//! - Workspaces (`workspaces` field, `pnpm-workspace.yaml`): each package with
//!   a dev script becomes its own process.
