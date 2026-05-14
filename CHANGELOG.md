# Changelog

## [0.10.1](https://github.com/lumpinif/agents-router/compare/v0.10.0...v0.10.1) (2026-05-14)


### Bug Fixes

* show session id after notification time ([3d53e1b](https://github.com/lumpinif/agents-router/commit/3d53e1b6ec166bd0a555ced8dc1e9362fbaaa1dd))

## [0.10.0](https://github.com/lumpinif/agents-router/compare/v0.9.1...v0.10.0) (2026-05-14)


### Features

* add delivery safety guard ([237d3c8](https://github.com/lumpinif/agents-router/commit/237d3c8a027d6ec03f8669854531ebf6f00849fa))
* reconcile hook-based source integrations ([6b61221](https://github.com/lumpinif/agents-router/commit/6b612214308c2a87f29c1535dd8b2f676d545af8))
* support explicit emit durations ([fc41f45](https://github.com/lumpinif/agents-router/commit/fc41f454e2eba1c1956d11d4eb98b6515a32ef19))


### Bug Fixes

* align setup provider ids with provider types ([098f466](https://github.com/lumpinif/agents-router/commit/098f4661e8f5ada6f61442f4ae07e4e0be0e1b1a))
* harden codex index and provider env urls ([d10f29d](https://github.com/lumpinif/agents-router/commit/d10f29df3cabf3068ac4f94c554fbb32e8c7109f))
* harden routing and service reliability ([3b8a6a4](https://github.com/lumpinif/agents-router/commit/3b8a6a44065087811d1b336e7238cc4817dc4cf8))
* ignore desktop-origin Codex CLI hooks ([00e3b96](https://github.com/lumpinif/agents-router/commit/00e3b96ae1862d1c0d30942d85cd190bc97c68e0))
* prevent Codex Desktop replay after provider failure ([377721c](https://github.com/lumpinif/agents-router/commit/377721ce8a5d6ff5f85ffb3171af82bf012183d4))

## [0.9.1](https://github.com/lumpinif/agents-router/compare/v0.9.0...v0.9.1) (2026-05-13)


### Bug Fixes

* preserve cargo-installed legacy binary during migration ([b17d90c](https://github.com/lumpinif/agents-router/commit/b17d90cee15d894afe1607fedc83ec75d927fec0))

## [0.9.0](https://github.com/lumpinif/agents-router/compare/v0.8.1...v0.9.0) (2026-05-13)


### Features

* rename project to Agents Router ([71207c0](https://github.com/lumpinif/agents-router/commit/71207c06f92a9edf9162e97059cdf309f6ba187f))

## 0.8.1

This release improves setup, agent hooks, and upgrades.

- Easier setup with notification preferences.
- Better support for structured CLI agent hooks.
- Optional filters for long-running tasks and selected projects.
- Config changes reload automatically when valid.
- Installers now restart the running service after upgrade.
- Weixin is now named WeChat.
- Fixed Codex Desktop model detection.
