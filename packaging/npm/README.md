# npm Packaging

This directory contains the npm launcher source for Agents Notifier.

The npm package does not rebuild the Rust project. It publishes a small Node.js
launcher plus platform-specific packages that contain the release binaries built
by GitHub Actions.

Prepare package directories from release archives:

```bash
node scripts/prepare-npm-release.js --dist dist --out dist/npm-packages
```

Prepare publishable tarballs:

```bash
node scripts/prepare-npm-release.js \
  --dist dist \
  --out dist/npm-packages \
  --pack-destination dist/npm-tarballs
```

Publish the native packages first, then publish the main package:

```bash
npm publish dist/npm-packages/agents-notifier-darwin-arm64
npm publish dist/npm-packages/agents-notifier-darwin-x64
npm publish dist/npm-packages/agents-notifier-linux-x64-gnu
npm publish dist/npm-packages/agents-notifier-win32-x64-msvc
npm publish dist/npm-packages/agents-notifier
```

The main package depends on the native packages at the exact same version, so
publishing the main package first will make fresh installs fail.
