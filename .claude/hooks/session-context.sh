#!/bin/bash
# SessionStart: a short picture of where the repo stands.
cd "${CLAUDE_PROJECT_DIR:-.}" || exit 0
echo "Paddock repo state:"
echo "  branch: $(git rev-parse --abbrev-ref HEAD 2>/dev/null)  last: $(git log -1 --format='%h %s' 2>/dev/null || echo 'no commits')"
echo "  uncommitted files: $(git status --short 2>/dev/null | wc -l | tr -d ' ')"
[ -f Cargo.toml ] || echo "  no Cargo.toml yet: the crate has not been scaffolded"
[ -d target ] && echo "  target/ cache present" || echo "  first cargo build will take a while"
ls docs/plans/*.md >/dev/null 2>&1 && echo "  open plans: $(ls docs/plans/*.md | xargs -n1 basename | tr '\n' ' ')"
exit 0
