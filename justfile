fmt:
    cargo fmt

fmt-check:
    cargo fmt --check

lint:
    cargo clippy -- -D warnings

test:
    cargo test

check:
    cargo fmt --check
    cargo clippy -- -D warnings
    cargo test

install-local:
    cargo install --path . --force

release version:
    ./scripts/release.sh {{version}}
