#!/usr/bin/env node
"use strict";

const { spawnSync } = require("node:child_process");
const fs = require("node:fs");
const path = require("node:path");

const REPO_ROOT = path.resolve(__dirname, "..");
const PACKAGE_ORDER = [
  "agents-notifier-darwin-arm64",
  "agents-notifier-darwin-x64",
  "agents-notifier-linux-x64-gnu",
  "agents-notifier-win32-x64-msvc",
  "agents-notifier",
];

function parseArgs(argv) {
  const args = {
    packages: path.join(REPO_ROOT, "dist", "npm-packages"),
  };

  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    const next = argv[index + 1];

    if (arg === "--packages" && next) {
      args.packages = path.resolve(next);
      index += 1;
    } else {
      throw new Error(`unsupported argument: ${arg}`);
    }
  }

  return args;
}

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    stdio: "inherit",
    ...options,
  });

  if (result.error) {
    throw result.error;
  }

  if (result.status !== 0) {
    throw new Error(`${command} ${args.join(" ")} failed`);
  }
}

function npmViewVersion(name, version) {
  const result = spawnSync("npm", ["view", `${name}@${version}`, "version"], {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  });

  if (result.status === 0 && result.stdout.trim() === version) {
    return version;
  }

  return null;
}

function readPackageJson(packageDir) {
  const packageJsonPath = path.join(packageDir, "package.json");
  const content = fs.readFileSync(packageJsonPath, "utf8");
  return JSON.parse(content);
}

function publishPackage(packageDir) {
  const packageJson = readPackageJson(packageDir);
  const existingVersion = npmViewVersion(packageJson.name, packageJson.version);

  if (existingVersion) {
    console.log(`${packageJson.name}@${packageJson.version} is already published; skipping.`);
    return;
  }

  const publishArgs = ["publish", packageDir];
  if (process.env.GITHUB_ACTIONS === "true") {
    publishArgs.push("--provenance");
  }

  run("npm", publishArgs);
}

function main() {
  const args = parseArgs(process.argv.slice(2));

  for (const packageName of PACKAGE_ORDER) {
    const packageDir = path.join(args.packages, packageName);
    if (!fs.existsSync(path.join(packageDir, "package.json"))) {
      throw new Error(`missing npm package directory: ${packageDir}`);
    }

    publishPackage(packageDir);
  }
}

try {
  main();
} catch (error) {
  console.error(`publish-npm-release: ${error.message}`);
  process.exit(1);
}
