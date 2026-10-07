#!/bin/bash
# PreToolUse (Bash): block commands that could hurt the user's machine or repo.
# Paddock's own tests start and stop processes, so broad kills are the main risk.
# Exit 2 = block, reason on stderr.
set -u
input=$(cat)
cmd=$(printf '%s' "$input" | python3 -c 'import json,sys; print(json.load(sys.stdin).get("tool_input",{}).get("command",""))' 2>/dev/null || true)
[ -z "$cmd" ] && exit 0

deny() { echo "Paddock guard: blocked, $1" >&2; exit 2; }

# Broad kills would take down the user's real dev servers and editors.
printf '%s' "$cmd" | grep -Eq '(^|[;&| ])killall( |$)' && deny "killall hits every matching process on the machine; kill the specific pid or process group you started"
printf '%s' "$cmd" | grep -Eq 'pkill +(-[a-zA-Z0-9]+ +)*(node|yarn|npm|pnpm|bun|cargo|python3?|ruby|docker|expo|vite|next)( |$)' && deny "pkill by a common program name; target the pid or process group your test started"
printf '%s' "$cmd" | grep -Eq 'kill +(-[a-zA-Z0-9]+ +)*-1( |$)' && deny "kill -1 signals every process you own"
# Never touch the user's other projects from here.
printf '%s' "$cmd" | grep -Eq 'rm +-[a-zA-Z]*r[a-zA-Z]* +(~|\$HOME|/Users/[^/ ]+)(/Projects)?/?( |$)' && deny "recursive delete of a home or projects folder"
# Releases and history.
printf '%s' "$cmd" | grep -Eq '(cargo +publish|gh +release +create)' && deny "publishing is a human action; releases come from CI"
printf '%s' "$cmd" | grep -Eq 'git +push[^|;&]*(--force|-f)( |$)' && deny "force-push; ask the user first"
printf '%s' "$cmd" | grep -Eq '(cat|less|head|tail|grep)[^|;&]*\.env' && deny "reading .env files"
exit 0
