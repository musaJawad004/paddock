#!/bin/bash
# Stop hook: do not end a turn while the crate fails `cargo check`.
# Runs only when Rust sources changed since the last check.
set -u
input=$(cat)
active=$(printf '%s' "$input" | python3 -c 'import json,sys; print("1" if json.load(sys.stdin).get("stop_hook_active") else "")' 2>/dev/null || true)
[ "$active" = "1" ] && exit 0
cd "${CLAUDE_PROJECT_DIR:-.}" || exit 0
[ -f Cargo.toml ] || exit 0
export PATH="$HOME/.cargo/bin:$PATH"
mkdir -p target
stamp=target/.paddock-stop-check
if [ -f "$stamp" ] && [ -z "$(find src tests Cargo.toml -newer "$stamp" 2>/dev/null | head -1)" ]; then
  exit 0
fi
out=$(cargo check --all-targets --quiet 2>&1)
if [ $? -ne 0 ]; then
  echo "cargo check failed. Fix before finishing:" >&2
  printf '%s\n' "$out" | grep -E '^error' -A 8 | head -50 >&2
  exit 2
fi
touch "$stamp"
exit 0
