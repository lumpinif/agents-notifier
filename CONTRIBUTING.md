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

## Commit Messages

Use Conventional Commits:

```text
<type>(optional-scope): short description
```

Allowed types:

- `feat`: user-visible feature or capability
- `fix`: user-visible bug fix
- `perf`: user-visible performance improvement
- `docs`: documentation only
- `test`: tests only
- `refactor`: code structure change with no user-visible behavior change
- `chore`: maintenance only
- `ci`: CI workflow change
- `build`: build, packaging, or dependency tooling
- `style`: formatting only

Version impact:

- `fix:` creates a patch release.
- `feat:` creates a minor release.
- `perf:` may appear in release notes when it matters to users.
- `docs:`, `test:`, `refactor:`, `chore:`, `ci:`, `build:`, and `style:` do not create a release by themselves.
- Breaking changes must use `!` or a `BREAKING CHANGE:` footer.

Examples:

```text
feat(setup): add notification duration preference
fix(install): restart running service after upgrade
docs: document WeChat setup
ci: add release-please workflow
```

Do not use vague messages such as `update`, `changes`, `misc`, or `wip`.

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

Releases are driven by Release Please.

Normal code pushes do not publish a release directly. After user-visible commits land on `main`, Release Please opens or updates a release PR.

The release PR contains:

- the next version
- `Cargo.toml`
- `Cargo.lock`
- `.release-please-manifest.json`
- `CHANGELOG.md`

Review the changelog before merging the release PR. Keep release notes short and user-facing.

When the release PR is merged, Release Please creates the version tag and GitHub Release. The tag-based Release workflow then builds release assets and publishes npm packages.

Repository setup requires a `RELEASE_PLEASE_TOKEN` secret. Use a PAT instead of the default `GITHUB_TOKEN` so release tags created by Release Please can trigger the tag-based Release workflow.

Manual release remains available only as a fallback:

```bash
just release 0.1.1
```

The manual release helper checks:

- current branch is `main`
- working tree is clean
- `Cargo.toml` version matches the release version
- local `main` matches `origin/main`
- local and remote tags do not already exist
- `just check` passes

Then it creates and pushes the tag.

GitHub Actions builds the release assets and publishes the GitHub Release.

Do not move or replace a public release tag.
