#!/bin/bash
# Try to fix dependency advisories found by cargo-deny.
#
# Runs `cargo update`, which only moves to semver-compatible versions, so it
# never changes Cargo.toml or makes a breaking upgrade. Then checks again.
# The caller (CI) decides what to do with the result; this script never
# commits or pushes.
#
# Prints result=<value> and, in CI, writes it to $GITHUB_OUTPUT:
#   clean       no advisories, nothing to do
#   fixed       Cargo.lock changed and advisories now pass
#   partial     Cargo.lock changed but some advisories remain
#   unfixable   cargo update changed nothing; needs a human
# Writes the list of updated crates to target/autofix-updates.txt.
set -u

emit() {
  echo "result=$1"
  [ -n "${GITHUB_OUTPUT:-}" ] && echo "result=$1" >> "$GITHUB_OUTPUT"
  exit 0
}

[ -f Cargo.toml ] || emit clean
mkdir -p target

if cargo deny --log-level error check advisories >/dev/null 2>&1; then
  emit clean
fi

cp Cargo.lock target/Cargo.lock.before 2>/dev/null || : > target/Cargo.lock.before
cargo update 2> target/autofix-cargo-update.log
grep -E '^[[:space:]]*(Updating|Adding|Removing) ' target/autofix-cargo-update.log \
  | grep -v 'crates.io index' | sed -E 's/^[[:space:]]+//' > target/autofix-updates.txt

if cmp -s Cargo.lock target/Cargo.lock.before; then
  emit unfixable
fi

if cargo deny --log-level error check advisories >/dev/null 2>&1; then
  emit fixed
fi
emit partial
