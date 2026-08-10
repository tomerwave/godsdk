#!/usr/bin/env node
"use strict";

const { spawnSync } = require("node:child_process");
const path = require("node:path");

function main() {
  const suffix = process.platform === "win32" ? ".exe" : "";
  const name = `@godsdk/cli-${process.platform}-${process.arch}`;
  let binary;

  try {
    binary = path.join(path.dirname(require.resolve(`${name}/package.json`)), `godsdk${suffix}`);
  } catch {
    console.error(
      `godsdk: no prebuilt binary for ${process.platform}-${process.arch}.`,
      "\nBuild from source with `cargo install godsdk-cli`."
    );
    return 1;
  }

  const finished = spawnSync(binary, process.argv.slice(2), { stdio: "inherit" });
  if (finished.error) {
    console.error(`godsdk: could not run ${binary}: ${finished.error.message}`);
    return 1;
  }
  return finished.status === null ? 1 : finished.status;
}

process.exitCode = main();
