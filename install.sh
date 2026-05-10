#!/bin/sh
set -eu

REPO="${AGENTS_NOTIFIER_REPO:-lumpinif/agents-notifier}"
INSTALL_DIR="${AGENTS_NOTIFIER_INSTALL_DIR:-$HOME/.local/bin}"

need() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Missing required command: $1" >&2
    exit 1
  fi
}

need curl
need tar
need shasum

os="$(uname -s)"
arch="$(uname -m)"

case "$os" in
  Darwin) ;;
  *)
    echo "Agents Notifier install script currently supports macOS only." >&2
    exit 1
    ;;
esac

case "$arch" in
  arm64) target="aarch64-apple-darwin" ;;
  x86_64) target="x86_64-apple-darwin" ;;
  *)
    echo "Unsupported macOS architecture: $arch" >&2
    exit 1
    ;;
esac

archive="agents-notifier-${target}.tar.gz"
base_url="https://github.com/${REPO}/releases/latest/download"
tmp_dir="$(mktemp -d)"

cleanup() {
  rm -rf "$tmp_dir"
}
trap cleanup EXIT INT TERM

echo "Downloading Agents Notifier for ${target}..."
curl -fsSL "${base_url}/${archive}" -o "${tmp_dir}/${archive}"
curl -fsSL "${base_url}/${archive}.sha256" -o "${tmp_dir}/${archive}.sha256"

(cd "$tmp_dir" && shasum -a 256 -c "${archive}.sha256")

tar -xzf "${tmp_dir}/${archive}" -C "$tmp_dir"
mkdir -p "$INSTALL_DIR"
cp "${tmp_dir}/agents-notifier" "${INSTALL_DIR}/agents-notifier"
chmod 0755 "${INSTALL_DIR}/agents-notifier"

echo "Installed: ${INSTALL_DIR}/agents-notifier"

case ":$PATH:" in
  *":$INSTALL_DIR:"*) ;;
  *)
    echo
    echo "Add this to your shell profile if agents-notifier is not found:"
    echo "  export PATH=\"$INSTALL_DIR:\$PATH\""
    ;;
esac

echo
echo "Next:"
echo "  agents-notifier setup"
