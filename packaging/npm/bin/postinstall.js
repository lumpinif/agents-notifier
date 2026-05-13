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

function pathParts(filePath) {
  return path.resolve(filePath).split(path.sep).filter(Boolean);
}

function pathIsInside(parent, child) {
  const relative = path.relative(path.resolve(parent), path.resolve(child));
  return relative === "" || (!relative.startsWith("..") && !path.isAbsolute(relative));
}

function isNpxCachePath(filePath) {
  const npmCache = process.env.npm_config_cache;
  if (npmCache && pathIsInside(path.join(npmCache, "_npx"), filePath)) {
    return true;
  }

  return pathParts(filePath).includes("_npx");
}

function nativeBinaryPath() {
  const platformKey = `${process.platform} ${process.arch}`;
  const packageName = PACKAGE_BY_PLATFORM[platformKey];
  if (!packageName) {
    return null;
  }

  let packageJsonPath;
  try {
    packageJsonPath = require.resolve(`${packageName}/package.json`);
  } catch {
    return null;
  }

  const binaryName =
    process.platform === "win32" ? "agents-notifier.exe" : "agents-notifier";
  const binaryPath = path.join(path.dirname(packageJsonPath), "bin", binaryName);
  return fs.existsSync(binaryPath) ? binaryPath : null;
}

function runNative(binaryPath, args, stdio) {
  return spawnSync(binaryPath, args, {
    encoding: stdio === "pipe" ? "utf8" : undefined,
    stdio,
    env: {
      ...process.env,
      AGENTS_NOTIFIER_INSTALL_METHOD: "npm",
    },
  });
}

function serviceIsRunning(binaryPath) {
  const result = runNative(binaryPath, ["status"], "pipe");
  if (result.error || result.status !== 0) {
    return false;
  }

  return /running:\s*yes/i.test(`${result.stdout || ""}\n${result.stderr || ""}`);
}

const binaryPath = nativeBinaryPath();
if (!binaryPath || isNpxCachePath(binaryPath) || !serviceIsRunning(binaryPath)) {
  process.exit(0);
}

console.error("agents-notifier: restarting existing service after npm install...");
const result = runNative(binaryPath, ["restart"], "inherit");
if (result.error) {
  console.error(`agents-notifier: failed to restart service: ${result.error.message}`);
  process.exit(1);
}

process.exit(result.status === null ? 1 : result.status);
