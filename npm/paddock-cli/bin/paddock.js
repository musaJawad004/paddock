#!/usr/bin/env node
// Runs the Paddock binary for this machine. npm installed it as one of the
// optional dependencies (paddock-cli-darwin-arm64 and so on), picking the
// one whose os and cpu match. Nothing is downloaded here.

"use strict";

const { spawnSync } = require("node:child_process");

const key = `${process.platform}-${process.arch}`;
const exe = process.platform === "win32" ? "paddock.exe" : "paddock";

let binary;
try {
  binary = require.resolve(`paddock-cli-${key}/bin/${exe}`);
} catch {
  console.error(
    `paddock: there is no build for ${key} yet. ` +
      "Install from source with: cargo install --git https://github.com/musaJawad004/paddock"
  );
  process.exit(1);
}

const result = spawnSync(binary, process.argv.slice(2), { stdio: "inherit" });
if (result.error) {
  console.error(`paddock: could not start ${binary}: ${result.error.message}`);
  process.exit(1);
}
process.exit(result.status === null ? 1 : result.status);
