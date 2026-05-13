# npm Packaging

This directory contains the npm launcher source for Agents Notifier.

The npm package does not rebuild the Rust project. It publishes a small Node.js
launcher plus platform-specific packages that contain the release binaries built
by GitHub Actions.

The launcher supports `npx --yes --prefer-online agents-notifier@latest setup`
by copying the native binary from the npx cache into a stable local install path
before running setup.
The service must never point at npm's temporary npx cache.
The main package postinstall restarts an already running service for persistent
npm installs only. It skips npx cache installs so the service is never pointed
at a temporary npx path.

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
node scripts/publish-npm-release.js --packages dist/npm-packages
```

The main package depends on the native packages at the exact same version, so
publishing the main package first will make fresh installs fail.

For the first manual publish after a GitHub Release already exists, use:

```bash
just npm-publish 0.6.0
```

GitHub Actions can publish with npm Trusted Publishing/OIDC or with a repository
secret named `NPM_TOKEN`. Trusted Publishing is preferred because it avoids a
long-lived publish token.
