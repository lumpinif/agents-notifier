#!/usr/bin/env node
"use strict";

const { spawnSync } = require("node:child_process");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");

const REPO_ROOT = path.resolve(__dirname, "..");
const LAUNCHER_SOURCE = path.join(
  REPO_ROOT,
  "packaging",
  "npm",
  "bin",
  "agents-router.js"
);
const POSTINSTALL_SOURCE = path.join(
  REPO_ROOT,
  "packaging",
  "npm",
  "bin",
  "postinstall.js"
);

const PLATFORMS = [
  {
    packageName: "agents-router-darwin-arm64",
    target: "aarch64-apple-darwin",
    archive: "agents-router-aarch64-apple-darwin.tar.gz",
    binaryName: "agents-router",
    os: ["darwin"],
    cpu: ["arm64"],
    label: "macOS arm64",
  },
  {
    packageName: "agents-router-darwin-x64",
    target: "x86_64-apple-darwin",
    archive: "agents-router-x86_64-apple-darwin.tar.gz",
    binaryName: "agents-router",
    os: ["darwin"],
    cpu: ["x64"],
    label: "macOS x64",
  },
  {
    packageName: "agents-router-linux-x64-gnu",
    target: "x86_64-unknown-linux-gnu",
    archive: "agents-router-x86_64-unknown-linux-gnu.tar.gz",
    binaryName: "agents-router",
    os: ["linux"],
    cpu: ["x64"],
    libc: ["glibc"],
    label: "Linux x64 GNU",
  },
  {
    packageName: "agents-router-win32-x64-msvc",
    target: "x86_64-pc-windows-msvc",
    archive: "agents-router-x86_64-pc-windows-msvc.zip",
    binaryName: "agents-router.exe",
    os: ["win32"],
    cpu: ["x64"],
    label: "Windows x64 MSVC",
  },
];

function parseArgs(argv) {
  const args = {
    dist: path.join(REPO_ROOT, "dist"),
    out: path.join(REPO_ROOT, "dist", "npm-packages"),
    packDestination: null,
    version: null,
  };

  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    const next = argv[index + 1];

    if (arg === "--dist" && next) {
      args.dist = path.resolve(next);
      index += 1;
    } else if (arg === "--out" && next) {
      args.out = path.resolve(next);
      index += 1;
    } else if (arg === "--pack-destination" && next) {
      args.packDestination = path.resolve(next);
      index += 1;
    } else if (arg === "--version" && next) {
      args.version = next;
      index += 1;
    } else {
      throw new Error(`unsupported argument: ${arg}`);
    }
  }

  return args;
}

function readCargoVersion() {
  const cargoToml = fs.readFileSync(path.join(REPO_ROOT, "Cargo.toml"), "utf8");
  let inPackageSection = false;

  for (const line of cargoToml.split(/\r?\n/)) {
    const section = line.match(/^\s*\[([^\]]+)\]\s*$/);
    if (section) {
      inPackageSection = section[1] === "package";
      continue;
    }

    if (!inPackageSection) {
      continue;
    }

    const version = line.match(/^\s*version\s*=\s*"([^"]+)"\s*$/);
    if (version) {
      return version[1];
    }
  }

  throw new Error("failed to read package version from Cargo.toml");
}

function writeJson(filePath, value) {
  fs.writeFileSync(filePath, `${JSON.stringify(value, null, 2)}\n`);
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

function resetDirectory(directory) {
  const resolved = path.resolve(directory);
  if (resolved === REPO_ROOT || resolved === path.dirname(REPO_ROOT)) {
    throw new Error(`refusing to remove unsafe output directory: ${resolved}`);
  }

  fs.rmSync(resolved, { recursive: true, force: true });
  fs.mkdirSync(resolved, { recursive: true });
}

function copyFile(source, destination, mode) {
  fs.mkdirSync(path.dirname(destination), { recursive: true });
  fs.copyFileSync(source, destination);
  if (mode !== undefined) {
    fs.chmodSync(destination, mode);
  }
}

function mainPackageJson(version) {
  return {
    name: "agents-router",
    version,
    description: "Local-first signal routing for AI coding agents",
    homepage: "https://github.com/lumpinif/agents-router",
    repository: {
      type: "git",
      url: "git+https://github.com/lumpinif/agents-router.git",
    },
    bugs: {
      url: "https://github.com/lumpinif/agents-router/issues",
    },
    keywords: [
      "agents",
      "notifications",
      "codex",
      "claude",
      "cli",
      "local-first",
    ],
    bin: {
      "agents-router": "bin/agents-router.js",
    },
    scripts: {
      postinstall: "node bin/postinstall.js",
    },
    engines: {
      node: ">=16",
    },
    files: ["bin"],
    optionalDependencies: Object.fromEntries(
      PLATFORMS.map((platform) => [platform.packageName, version])
    ),
  };
}

function nativePackageJson(platform, version) {
  return {
    name: platform.packageName,
    version,
    description: `Native agents-router binary for ${platform.label}`,
    homepage: "https://github.com/lumpinif/agents-router",
    repository: {
      type: "git",
      url: "git+https://github.com/lumpinif/agents-router.git",
    },
    bugs: {
      url: "https://github.com/lumpinif/agents-router/issues",
    },
    os: platform.os,
    cpu: platform.cpu,
    ...(platform.libc ? { libc: platform.libc } : {}),
    files: ["bin"],
  };
}

function extractArchive(archivePath, platform, destination) {
  const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), "agents-router-npm-"));
  try {
    if (archivePath.endsWith(".tar.gz")) {
      run("tar", ["-xzf", archivePath, "-C", tempDir]);
    } else if (archivePath.endsWith(".zip")) {
      run("unzip", ["-q", archivePath, "-d", tempDir]);
    } else {
      throw new Error(`unsupported archive type: ${archivePath}`);
    }

    const extractedBinary = path.join(tempDir, platform.binaryName);
    if (!fs.existsSync(extractedBinary)) {
      throw new Error(`archive did not contain ${platform.binaryName}: ${archivePath}`);
    }

    const mode = platform.binaryName.endsWith(".exe") ? undefined : 0o755;
    copyFile(extractedBinary, destination, mode);
  } finally {
    fs.rmSync(tempDir, { recursive: true, force: true });
  }
}

function prepareMainPackage(outDir, version) {
  const packageDir = path.join(outDir, "agents-router");
  fs.mkdirSync(packageDir, { recursive: true });
  writeJson(path.join(packageDir, "package.json"), mainPackageJson(version));
  copyFile(
    LAUNCHER_SOURCE,
    path.join(packageDir, "bin", "agents-router.js"),
    0o755
  );
  copyFile(
    POSTINSTALL_SOURCE,
    path.join(packageDir, "bin", "postinstall.js"),
    0o755
  );

  const readmePath = path.join(REPO_ROOT, "README.md");
  if (fs.existsSync(readmePath)) {
    copyFile(readmePath, path.join(packageDir, "README.md"));
  }

  return packageDir;
}

function prepareNativePackage(outDir, distDir, platform, version) {
  const archivePath = path.join(distDir, platform.archive);
  if (!fs.existsSync(archivePath)) {
    throw new Error(`missing release archive: ${archivePath}`);
  }

  const packageDir = path.join(outDir, platform.packageName);
  const binaryPath = path.join(packageDir, "bin", platform.binaryName);

  fs.mkdirSync(packageDir, { recursive: true });
  writeJson(path.join(packageDir, "package.json"), nativePackageJson(platform, version));
  extractArchive(archivePath, platform, binaryPath);

  return packageDir;
}

function packPackages(packageDirs, packDestination) {
  fs.mkdirSync(packDestination, { recursive: true });
  for (const packageDir of packageDirs) {
    run("npm", ["pack", packageDir, "--pack-destination", packDestination]);
  }
}

function printPublishCommands(packageDirs) {
  console.log("");
  console.log("Publish native packages first, then the main package:");
  for (const packageDir of packageDirs) {
    const relativePath = path.relative(REPO_ROOT, packageDir);
    const displayPath =
      relativePath && !relativePath.startsWith("..") && !path.isAbsolute(relativePath)
        ? relativePath
        : packageDir;
    console.log(`  npm publish ${displayPath}`);
  }
}

function main() {
  const args = parseArgs(process.argv.slice(2));
  const version = args.version ?? readCargoVersion();

  resetDirectory(args.out);

  const nativePackageDirs = PLATFORMS.map((platform) =>
    prepareNativePackage(args.out, args.dist, platform, version)
  );
  const mainPackageDir = prepareMainPackage(args.out, version);
  const packageDirs = [...nativePackageDirs, mainPackageDir];

  if (args.packDestination) {
    resetDirectory(args.packDestination);
    packPackages(packageDirs, args.packDestination);
    console.log(`Prepared npm package tarballs in ${args.packDestination}`);
  }

  console.log(`Prepared npm package directories in ${args.out}`);
  printPublishCommands(packageDirs);
}

try {
  main();
} catch (error) {
  console.error(`prepare-npm-release: ${error.message}`);
  process.exit(1);
}
