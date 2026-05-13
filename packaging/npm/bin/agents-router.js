#!/usr/bin/env node
"use strict";

const { spawnSync } = require("node:child_process");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");

const PACKAGE_BY_PLATFORM = {
  "darwin arm64": "agents-router-darwin-arm64",
  "darwin x64": "agents-router-darwin-x64",
  "linux x64": "agents-router-linux-x64-gnu",
  "win32 x64": "agents-router-win32-x64-msvc",
};

const STABLE_INSTALL_COMMANDS = new Set(["setup", "start", "watch"]);
const STABLE_FORWARD_COMMANDS = new Set(["status", "stop", "uninstall"]);
const INSTALL_METHOD_MARKER_FILE = ".agents-router-install-method";

function fail(message) {
  console.error(`agents-router: ${message}`);
  process.exit(1);
}

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

function stableInstallPath(binaryName) {
  const configuredInstallDir = process.env.AGENTS_ROUTER_INSTALL_DIR;
  if (configuredInstallDir && configuredInstallDir.trim() !== "") {
    return path.join(configuredInstallDir, binaryName);
  }

  if (process.platform === "win32") {
    const localAppData = process.env.LOCALAPPDATA;
    if (!localAppData) {
      fail("LOCALAPPDATA is not set. Set AGENTS_ROUTER_INSTALL_DIR to choose an install directory.");
    }

    return path.join(localAppData, "Programs", "agents-router", binaryName);
  }

  const home = os.homedir();
  if (!home) {
    fail("home directory is not available. Set AGENTS_ROUTER_INSTALL_DIR to choose an install directory.");
  }

  return path.join(home, ".local", "bin", binaryName);
}

function sameFilePath(left, right) {
  return path.resolve(left) === path.resolve(right);
}

function spawnNative(binaryPath, args, installMethod) {
  return spawnSync(binaryPath, args, {
    stdio: "inherit",
    env: {
      ...process.env,
      AGENTS_ROUTER_INSTALL_METHOD: installMethod,
    },
  });
}

function captureNative(binaryPath, args, installMethod) {
  return spawnSync(binaryPath, args, {
    encoding: "utf8",
    env: {
      ...process.env,
      AGENTS_ROUTER_INSTALL_METHOD: installMethod,
    },
  });
}

function exitFromResult(result, binaryPath) {
  if (result.error) {
    fail(`failed to start native binary at ${binaryPath}: ${result.error.message}`);
  }

  if (result.signal) {
    fail(`native binary exited from signal ${result.signal}`);
  }

  process.exit(result.status === null ? 1 : result.status);
}

function copyNativeBinary(sourcePath, destinationPath) {
  fs.mkdirSync(path.dirname(destinationPath), { recursive: true });
  const tempPath = path.join(
    path.dirname(destinationPath),
    `.${path.basename(destinationPath)}.${process.pid}.${Date.now()}.tmp`
  );

  try {
    // Byte-copy through a new file so macOS npm cache provenance xattrs are not
    // carried onto the executable that launch services will run.
    fs.writeFileSync(tempPath, fs.readFileSync(sourcePath), {
      mode: process.platform === "win32" ? 0o666 : 0o755,
    });

    if (process.platform !== "win32") {
      fs.chmodSync(tempPath, 0o755);
    }

    if (process.platform === "win32") {
      fs.rmSync(destinationPath, { force: true });
    }

    fs.renameSync(tempPath, destinationPath);
  } catch (error) {
    fs.rmSync(tempPath, { force: true });
    throw error;
  }
}

function stopExistingStableService(destinationPath) {
  const result = spawnNative(destinationPath, ["stop"], "npx");
  if (result.error || result.status !== 0) {
    return false;
  }

  return true;
}

function stableServiceIsRunning(destinationPath, installMethod) {
  if (!fs.existsSync(destinationPath)) {
    return false;
  }

  const result = captureNative(destinationPath, ["status"], installMethod);
  if (result.error || result.status !== 0) {
    return false;
  }

  return /running:\s*yes/i.test(`${result.stdout || ""}\n${result.stderr || ""}`);
}

function stopStableServiceOrFail(destinationPath, installMethod) {
  const result = spawnNative(destinationPath, ["stop"], installMethod);
  if (result.error) {
    fail(`failed to stop existing service before install: ${result.error.message}`);
  }

  if (result.status !== 0) {
    fail(`existing service stop command exited with code ${result.status}`);
  }
}

function installStableBinary(sourcePath, destinationPath) {
  if (sameFilePath(sourcePath, destinationPath)) {
    return;
  }

  try {
    copyNativeBinary(sourcePath, destinationPath);
  } catch (error) {
    const canRetryAfterStop =
      process.platform === "win32" &&
      fs.existsSync(destinationPath) &&
      (error.code === "EACCES" || error.code === "EPERM");

    if (!canRetryAfterStop || !stopExistingStableService(destinationPath)) {
      throw error;
    }

    copyNativeBinary(sourcePath, destinationPath);
  }
}

function writeInstallMethodMarker(destinationPath, installMethod) {
  fs.writeFileSync(
    path.join(path.dirname(destinationPath), INSTALL_METHOD_MARKER_FILE),
    `${installMethod}\n`
  );
}

function printPathHint(destinationPath) {
  const installDir = path.dirname(destinationPath);
  const pathEntries = (process.env.PATH || "")
    .split(path.delimiter)
    .filter(Boolean)
    .map((entry) => path.resolve(entry));

  if (pathEntries.includes(path.resolve(installDir))) {
    return;
  }

  console.error(`Installed stable agents-router binary: ${destinationPath}`);
  console.error(`Add this directory to PATH if you want to run agents-router directly: ${installDir}`);
  console.error("");
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
    `missing native package ${packageName}. Reinstall with "npm install -g agents-router".`
  );
}

const binaryName =
  process.platform === "win32" ? "agents-router.exe" : "agents-router";
const binaryPath = path.join(path.dirname(packageJsonPath), "bin", binaryName);

if (!fs.existsSync(binaryPath)) {
  fail(`native binary is missing: ${binaryPath}`);
}

const args = process.argv.slice(2);
const command = args[0] || "";
const installMethod = isNpxCachePath(binaryPath) ? "npx" : "npm";

if (installMethod === "npx" && STABLE_INSTALL_COMMANDS.has(command)) {
  const stablePath = stableInstallPath(binaryName);
  if (command === "start" && stableServiceIsRunning(stablePath, "npx")) {
    stopStableServiceOrFail(stablePath, "npx");
  }

  try {
    installStableBinary(binaryPath, stablePath);
    writeInstallMethodMarker(stablePath, "npx");
  } catch (error) {
    fail(`failed to install stable binary at ${stablePath}: ${error.message}`);
  }

  printPathHint(stablePath);
  exitFromResult(spawnNative(stablePath, args, "npx"), stablePath);
}

if (installMethod === "npx" && STABLE_FORWARD_COMMANDS.has(command)) {
  const stablePath = stableInstallPath(binaryName);
  if (fs.existsSync(stablePath)) {
    exitFromResult(spawnNative(stablePath, args, "npx"), stablePath);
  }
}

exitFromResult(spawnNative(binaryPath, args, installMethod), binaryPath);
