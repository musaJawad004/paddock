#!/bin/bash
# PostToolUse (Edit|Write): reject em and en dashes in files we author.
# Vendored skills under .claude/skills/ are third-party text and are skipped.
# Exit 2 sends the offending lines back to Claude to fix.
set -u
input=$(cat)
file=$(printf '%s' "$input" | python3 -c 'import json,sys; print(json.load(sys.stdin).get("tool_input",{}).get("file_path") or "")' 2>/dev/null || true)
[ -z "$file" ] || [ ! -f "$file" ] && exit 0
case "$file" in
  */.claude/skills/*) [ "$(basename "$(dirname "$file")")" = "ratatui-tui" ] || exit 0 ;;
  */target/*|*.lock) exit 0 ;;
esac
hits=$(grep -n $'\xe2\x80\x94\|\xe2\x80\x93' "$file" | head -10)
[ -z "$hits" ] && exit 0
echo "Em/en dashes are not allowed (see .claude/rules/writing-style.md). Rewrite these lines in $file:" >&2
printf '%s\n' "$hits" >&2
exit 2
