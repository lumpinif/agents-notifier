# AGENTS.md

## Mission

Build a small, durable, local-first system with clean boundaries, strong reliability, and low operational weight.

This repository should feel minimal on the surface and disciplined underneath: simple interfaces, explicit behavior, observable execution, and architecture that can expand without becoming heavy.

## Read First

Before changing code, read:

1. `DEVELOPING/README.md`
2. `DEVELOPING/spec/phase-1-signal-forwarder.md`
3. `DEVELOPING/engineering/README.md`
4. The engineering document for the area you will touch.

## Architecture

Keep core boundaries clean:

```text
Source Adapter -> Signal -> Router -> Provider Adapter
```

Rules:

- Source adapters create `Signal`.
- Providers consume `Signal`.
- Router connects signals to providers through config.
- Sources do not know providers.
- Providers do not know sources.
- Core logic does not depend on adapter metadata.
- The core must stay agent-agnostic.

## Development Style

- Make the simple path correct.
- Prefer explicit code over clever code.
- Keep files focused.
- Keep names literal and stable.
- Keep user-facing behavior predictable.
- Fail fast on invalid config.
- Preserve error context.
- Do not swallow errors.
- Do not add fallback behavior unless it is explicitly required.
- Add abstractions only for real boundaries or real duplication.
- Prefer long-term structural correctness over short-term patches.
- Do not let adapters leak into core concepts.

## Reliability

- Keep failure modes visible.
- Keep local behavior controllable.
- Avoid hidden background work.
- Avoid reading private internals unless the spec explicitly requires it.
- Do not hide provider, IO, parser, or config failures.

## Observability

- Use `tracing`.
- Log key boundaries: config, source, router, provider, shutdown.
- Include `signal.id` on signal-related logs.
- Never log tokens, API keys, webhook secrets, full webhook URLs, full code content, or long raw output.

## Testing

- Test behavior, not trivia.
- Bug fixes require Red-Green TDD.
- Do not write tests that always pass.
- Do not change product behavior only to satisfy tests.
- Do not use real external services in unit tests.
- Keep fixtures small, real-shaped, and sanitized.

## Commands

- Use `cargo` for Rust build, test, and dependency management.
- Use `justfile` as the minimal command entrypoint.
- `justfile` must not become a second build system.

## Git

- Use commits to keep the development path clear.
- Commit within clear boundaries.
- Do not mix unrelated changes.
- Never push to a remote unless the user explicitly asks for it.
- If the user asks for a commit, treat it as a local commit only.

## Documentation

- Product scope belongs in `DEVELOPING/spec/`.
- Engineering standards belong in `DEVELOPING/engineering/`.
- Keep documentation short, precise, and useful for agents.
- Update documentation when the implemented contract changes.
