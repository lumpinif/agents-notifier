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

package_version="$(cargo metadata --no-deps --format-version 1 \
  | sed -n 's/.*"name":"agents-notifier","version":"\([^"]*\)".*/\1/p')"

if [[ "$package_version" != "$version" ]]; then
  echo "Cargo.toml version is ${package_version}, expected ${version}."
  exit 1
fi

git fetch origin main --tags

local_main="$(git rev-parse main)"
remote_main="$(git rev-parse origin/main)"

if [[ "$local_main" != "$remote_main" ]]; then
  echo "Local main is not equal to origin/main."
  echo "Push or pull before releasing."
  exit 1
fi

if git rev-parse "$tag" >/dev/null 2>&1; then
  echo "Local tag already exists: $tag"
  exit 1
fi

if git ls-remote --exit-code --tags origin "refs/tags/${tag}" >/dev/null 2>&1; then
  echo "Remote tag already exists: $tag"
  exit 1
fi

just check

git tag "$tag"
git push origin "$tag"

echo "Release tag pushed: $tag"
echo "GitHub Actions will build and publish the release."
