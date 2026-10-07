# Git and workflow

## How a feature gets built

1. `brainstorming` skill for anything with open design questions.
2. `writing-plans` for work longer than an hour. Plans live in `docs/plans/`.
3. `test-driven-development`: failing test first for logic (server
   grouping, port parsing, update functions).
4. `systematic-debugging` when something breaks. Find the cause before
   changing code.
5. `verification-before-completion`: run the commands and look at the output
   before saying it works.
6. `requesting-code-review` (or the `rust-reviewer` agent) before merging to
   `main`.

## Commits

- Small and focused. Subject in imperative mood, 72 characters max, no
  trailing full stop: `Detect package manager from lockfile`.
- Body explains why when the diff does not make it obvious.
- No em or en dashes, no emoji (see `writing-style.md`).
- `main` must always build: `cargo fmt --check`, `cargo clippy`, `cargo test`.
- Never commit `target/`, `.env*`, editor folders, or anything from the
  user's home directory.
- Never force-push `main`. Never publish a crate or release from a local
  machine; releases come from CI.

## Versioning

- SemVer. Version lives in `Cargo.toml` only.
- User-visible changes get a line in `CHANGELOG.md` under `Unreleased`.
