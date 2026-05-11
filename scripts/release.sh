#!/usr/bin/env bash
set -euo pipefail

version="${1:-}"

if [[ -z "$version" ]]; then
  echo "Usage: just release <version>"
  echo "Example: just release 0.1.1"
  exit 2
fi

if [[ ! "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "Version must look like 0.1.1"
  exit 2
fi

tag="v${version}"

if [[ "$(git rev-parse --abbrev-ref HEAD)" != "main" ]]; then
  echo "Release must run from the main branch."
  exit 1
fi

if [[ -n "$(git status --porcelain)" ]]; then
  echo "Release requires a clean working tree."
  echo "Commit or stash your changes first."
  exit 1
fi

git fetch origin main --tags

local_main="$(git rev-parse main)"
remote_main="$(git rev-parse origin/main)"

if git rev-parse "$tag" >/dev/null 2>&1; then
  echo "Local tag already exists: $tag"
  exit 1
fi

if git ls-remote --exit-code --tags origin "refs/tags/${tag}" >/dev/null 2>&1; then
  echo "Remote tag already exists: $tag"
  exit 1
fi

package_version="$(cargo metadata --no-deps --format-version 1 \
  | sed -n 's/.*"name":"agents-notifier","version":"\([^"]*\)".*/\1/p')"

if [[ "$package_version" != "$version" ]]; then
  if [[ "$local_main" != "$remote_main" ]]; then
    echo "Local main must equal origin/main before the release version bump."
    echo "Push or pull before releasing."
    exit 1
  fi

  tmp_file="$(mktemp)"
  awk -v version="$version" '
    BEGIN { in_package = 0; changed = 0 }
    /^\[package\][[:space:]]*$/ {
      in_package = 1
      print
      next
    }
    /^\[/ && $0 !~ /^\[package\][[:space:]]*$/ {
      in_package = 0
    }
    in_package && /^[[:space:]]*version[[:space:]]*=/ && changed == 0 {
      sub(/"[^"]*"/, "\"" version "\"")
      changed = 1
    }
    { print }
    END {
      if (changed == 0) {
        exit 42
      }
    }
  ' Cargo.toml > "$tmp_file" || {
    rm -f "$tmp_file"
    echo "Failed to update Cargo.toml package version."
    exit 1
  }
  mv "$tmp_file" Cargo.toml

  cargo check

  package_version="$(cargo metadata --no-deps --format-version 1 \
    | sed -n 's/.*"name":"agents-notifier","version":"\([^"]*\)".*/\1/p')"

  if [[ "$package_version" != "$version" ]]; then
    echo "Cargo.toml version is ${package_version}, expected ${version}."
    exit 1
  fi
fi

just check

changed_files="$(git status --porcelain)"
if [[ -n "$changed_files" ]]; then
  unexpected_changes="$(printf '%s\n' "$changed_files" \
    | awk '$2 != "Cargo.toml" && $2 != "Cargo.lock" { print }')"
  if [[ -n "$unexpected_changes" ]]; then
    echo "Release checks changed unexpected files."
    git status --short
    exit 1
  fi

  git add Cargo.toml Cargo.lock
  git commit -m "Release ${version}"
fi

local_main="$(git rev-parse main)"
remote_main="$(git rev-parse origin/main)"

if [[ "$local_main" != "$remote_main" ]]; then
  if git merge-base --is-ancestor origin/main main; then
    git push origin main
  else
    echo "Local main and origin/main have diverged."
    echo "Pull or rebase before releasing."
    exit 1
  fi
fi

if [[ -n "$(git status --porcelain)" ]]; then
  echo "Release requires a clean working tree before tagging."
  git status --short
  exit 1
fi

git tag "$tag"
git push origin "$tag"

echo "Release tag pushed: $tag"
echo "GitHub Actions will build and publish the release."
