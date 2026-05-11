#!/usr/bin/env node
"use strict";

const { spawnSync } = require("node:child_process");
const fs = require("node:fs");
const path = require("node:path");

const PACKAGE_BY_PLATFORM = {
  "darwin arm64": "agents-notifier-darwin-arm64",
  "darwin x64": "agents-notifier-darwin-x64",
  "linux x64": "agents-notifier-linux-x64-gnu",
  "win32 x64": "agents-notifier-win32-x64-msvc",
};

function fail(message) {
  console.error(`agents-notifier: ${message}`);
  process.exit(1);
}

const platformKey = `${process.platform} ${process.arch}`;
const packageName = PACKAGE_BY_PLATFORM[platformKey];

if (!packageName) {
  fail(`unsupported platform ${process.platform} ${process.arch}`);
}

let packageJsonPath;
try {
  packageJsonPath = require.resolve(`${packageName}/package.json`);
} catch (error) {
  fail(
    `missing native package ${packageName}. Reinstall with "npm install -g agents-notifier".`
  );
}

const binaryName =
  process.platform === "win32" ? "agents-notifier.exe" : "agents-notifier";
const binaryPath = path.join(path.dirname(packageJsonPath), "bin", binaryName);

if (!fs.existsSync(binaryPath)) {
  fail(`native binary is missing: ${binaryPath}`);
}

const result = spawnSync(binaryPath, process.argv.slice(2), {
  stdio: "inherit",
  env: {
    ...process.env,
    AGENTS_NOTIFIER_INSTALL_METHOD: "npm",
  },
});

if (result.error) {
  fail(`failed to start native binary: ${result.error.message}`);
}

if (result.signal) {
  fail(`native binary exited from signal ${result.signal}`);
}

process.exit(result.status === null ? 1 : result.status);
