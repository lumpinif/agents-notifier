# Contributing

Thanks for helping improve Agents Notifier.

Keep the project small, local-first, and easy to trust.

## Principles

- Keep user data local.
- Keep behavior explicit.
- Keep errors visible.
- Keep the core simple: `Source -> Signal -> Router -> Provider`.
- Do not make providers depend on specific agents.
- Do not add cloud services, telemetry, or hidden background behavior without a clear product decision.

## Development

Run the checks before sending changes:

```bash
just check
```

If `just` is not installed:

```bash
cargo fmt --check
cargo clippy -- -D warnings
cargo test
```

Use focused commits. Do not mix unrelated changes.

## User-Facing Copy

User-facing product copy must be clear, short, and natural English.

Avoid internal terms when a user-facing term is clearer:

- Use "agent" in product copy.
- Use "source" only for config, code, or architecture.
- Use "provider" only when talking about notification delivery targets.

## Privacy

Do not log secrets, webhook tokens, full session content, prompts, tool output, or code content.

Codex Desktop support may read local completion metadata needed for a notification. Keep that data bounded and intentional.

## Release

Releases are tag-driven.

Normal code pushes do not publish a release. A release starts only when a `v*.*.*` tag is pushed.

Before releasing, update `Cargo.toml` to the target version and commit the change.

Then run:

```bash
just release 0.1.1
```

The release helper checks:

- current branch is `main`
- working tree is clean
- `Cargo.toml` version matches the release version
- local `main` matches `origin/main`
- local and remote tags do not already exist
- `just check` passes

Then it creates and pushes the tag.

GitHub Actions builds the macOS release assets and publishes the GitHub Release.

Do not move or replace a public release tag.
