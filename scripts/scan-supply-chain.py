#!/usr/bin/env python3
"""Scan the repository for injected payloads and for network code.

Paddock makes two promises, so there are two checks:

1. Nothing malicious has been injected into the tree or into any branch on
   the remote. Python rather than grep, because these payloads are often one
   line tens of thousands of characters long, which grep can fail to match.
   Font files are checked against their magic bytes, since a payload renamed
   to .woff2 would otherwise be skipped as a binary asset.

2. Paddock has no network code. A patch that quietly adds an HTTP client or
   a TCP socket fails the build the same way malware does. Rust test code is
   exempt: everything from the first `#[cfg(test)]` line to the end of a file
   is not checked for network use.

Signatures live in scripts/scan-signatures.tsv. That file is the only one the
malware check skips, because it would match itself. Dependency checks
(advisories, licenses, banned crates) are done by cargo-deny, see deny.toml.

    scripts/scan-supply-chain.py            # working tree and every origin branch
    scripts/scan-supply-chain.py --local    # working tree only, used by the pre-commit hook

Exit codes: 0 clean, 1 findings, 2 broken signature file.
"""

import os
import re
import subprocess
import sys

SIGNATURES = "scripts/scan-signatures.tsv"

SCAN_EXT = (
    ".rs", ".toml", ".py", ".sh", ".ps1", ".yml", ".yaml", ".json",
    ".js", ".cjs", ".mjs", ".ts", ".html", ".css", ".svg",
    ".woff", ".woff2", ".ttf", ".otf", ".eot",
)

SKIP = ("/target/", "/.git/", "/node_modules/", ".min.", "Cargo.lock")

FONT_MAGIC = {
    ".woff": (b"wOFF",),
    ".woff2": (b"wOF2",),
    ".ttf": (b"\x00\x01\x00\x00", b"true", b"ttcf"),
    ".otf": (b"OTTO", b"\x00\x01\x00\x00"),
}


def load_signatures(path=SIGNATURES):
    malware, network = [], []
    with open(path, encoding="utf-8") as fh:
        for number, line in enumerate(fh, 1):
            line = line.rstrip("\n")
            if not line.strip() or line.startswith("#"):
                continue
            parts = line.split("\t", 2)
            if len(parts) != 3 or parts[0] not in ("malware", "network"):
                sys.exit(f"{path}:{number}: expected kind<TAB>name<TAB>regex")
            kind, name, pattern = parts
            try:
                compiled = re.compile(pattern)
            except re.error as err:
                print(f"{path}:{number}: bad regex: {err}", file=sys.stderr)
                sys.exit(2)
            (malware if kind == "malware" else network).append((compiled, name))
    return malware, network


def font_mismatch(path, data):
    expected = FONT_MAGIC.get(os.path.splitext(path)[1].lower())
    if expected and data and not any(data.startswith(m) for m in expected):
        return "font extension but not font data"
    return None


def without_tests(text):
    cut = text.find("#[cfg(test)]")
    return text if cut == -1 else text[:cut]


def scan_file(path, data, malware, network):
    hits = []
    mismatch = font_mismatch(path, data)
    if mismatch:
        hits.append(mismatch)
    if b"\0" in data[:4096]:
        return hits
    text = data.decode("utf-8", errors="ignore")
    if path != SIGNATURES:
        hits += [name for pattern, name in malware if pattern.search(text)]
    if path.endswith(".rs") and "/tests/" not in "/" + path:
        code = without_tests(text)
        hits += [f"network code: {name}" for pattern, name in network if pattern.search(code)]
    return hits


def tracked_files(ref="HEAD"):
    out = subprocess.run(
        ["git", "ls-tree", "-r", "--name-only", ref],
        capture_output=True, text=True, check=False,
    ).stdout
    return [f for f in out.splitlines()
            if f.endswith(SCAN_EXT) and not any(s in "/" + f for s in SKIP)]


def staged_and_tracked_files():
    """Working tree files git knows about, including newly added ones."""
    out = subprocess.run(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard"],
        capture_output=True, text=True, check=False,
    ).stdout
    return [f for f in out.splitlines()
            if f.endswith(SCAN_EXT) and not any(s in "/" + f for s in SKIP)]


def blob(ref, path):
    result = subprocess.run(["git", "show", f"{ref}:{path}"], capture_output=True, check=False)
    return result.stdout if result.returncode == 0 else b""


def remote_branches():
    out = subprocess.run(
        ["git", "for-each-ref", "--format=%(refname:short)", "refs/remotes/origin"],
        capture_output=True, text=True, check=False,
    ).stdout
    return [ref for ref in out.split() if not ref.endswith("/HEAD") and ref != "origin"]


def report(findings, scope):
    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if not findings:
        print(f"Clean: no injected payloads and no network code in the {scope}.")
        if summary:
            with open(summary, "a", encoding="utf-8") as fh:
                fh.write(f"## Supply chain scan: clean\n\nChecked the {scope}.\n")
        return 0

    print("Findings:\n")
    for where, path, hits in findings:
        print(f"  {where}: {path}\n      {', '.join(hits)}")
        print(f"::error file={path}::{', '.join(hits)}")
    if summary:
        with open(summary, "a", encoding="utf-8") as fh:
            fh.write("## Supply chain scan: findings\n\n")
            for where, path, hits in findings:
                fh.write(f"- `{path}` ({where}): {', '.join(hits)}\n")
    return 1


def main():
    local_only = "--local" in sys.argv
    malware, network = load_signatures()
    findings = []

    for path in staged_and_tracked_files():
        if os.path.isfile(path):
            with open(path, "rb") as fh:
                hits = scan_file(path, fh.read(), malware, network)
            if hits:
                findings.append(("working tree", path, hits))

    if not local_only:
        for ref in remote_branches():
            for path in tracked_files(ref):
                hits = scan_file(path, blob(ref, path), malware, network)
                if hits:
                    findings.append((ref, path, hits))

    scope = "working tree" if local_only else "working tree and every origin branch"
    sys.exit(report(findings, scope))


if __name__ == "__main__":
    main()
