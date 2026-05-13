#!/usr/bin/env node
"use strict";

const { spawnSync } = require("node:child_process");
const fs = require("node:fs");
const path = require("node:path");

const REPO_ROOT = path.resolve(__dirname, "..");

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

function usage() {
  console.error("Usage: just npm-publish <version>");
  console.error("Example: just npm-publish 0.6.0");
}

function normalizeVersion(input) {
  const version = input?.startsWith("v") ? input.slice(1) : input;
  if (!version || !/^[0-9]+\.[0-9]+\.[0-9]+$/.test(version)) {
    usage();
    process.exit(2);
  }

  return version;
}

function resetDirectory(directory) {
  const resolved = path.resolve(directory);
  if (resolved === REPO_ROOT || resolved === path.dirname(REPO_ROOT)) {
    throw new Error(`refusing to remove unsafe output directory: ${resolved}`);
  }

  fs.rmSync(resolved, { recursive: true, force: true });
  fs.mkdirSync(resolved, { recursive: true });
}

function main() {
  const version = normalizeVersion(process.argv[2]);
  const tag = `v${version}`;
  const workDir = path.join(REPO_ROOT, "dist", "npm-publish", tag);
  const releaseAssetsDir = path.join(workDir, "release-assets");
  const packagesDir = path.join(workDir, "packages");
  const tarballsDir = path.join(workDir, "tarballs");

  run("npm", ["whoami"]);
  run("gh", ["release", "view", tag]);

  resetDirectory(workDir);
  fs.mkdirSync(releaseAssetsDir, { recursive: true });

  run("gh", [
    "release",
    "download",
    tag,
    "--dir",
    releaseAssetsDir,
    "--pattern",
    "agents-router-*",
  ]);

  run("node", [
    path.join("scripts", "prepare-npm-release.js"),
    "--dist",
    releaseAssetsDir,
    "--out",
    packagesDir,
    "--pack-destination",
    tarballsDir,
    "--version",
    version,
  ]);

  run("node", [
    path.join("scripts", "publish-npm-release.js"),
    "--packages",
    packagesDir,
  ]);

  console.log("");
  console.log(`npm packages for ${tag} are published.`);
  console.log(`Tarballs were prepared in ${tarballsDir}`);
  console.log("");
  console.log("Smoke check:");
  console.log("  npm view agents-router version");
  console.log("  npm install -g agents-router");
}

try {
  main();
} catch (error) {
  console.error(`publish-npm-local: ${error.message}`);
  process.exit(1);
}
