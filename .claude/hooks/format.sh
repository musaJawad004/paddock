#!/bin/bash
# PostToolUse (Edit|Write): rustfmt the touched Rust file.
set -u
input=$(cat)
file=$(printf '%s' "$input" | python3 -c 'import json,sys; print(json.load(sys.stdin).get("tool_input",{}).get("file_path") or "")' 2>/dev/null || true)
[ -z "$file" ] || [ ! -f "$file" ] && exit 0
export PATH="$HOME/.cargo/bin:$PATH"
case "$file" in
  *.rs) rustfmt --edition 2024 "$file" >/dev/null 2>&1 ;;
esac
exit 0
