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

verify_checksum() {
  archive="$1"
  checksum_file="$2"
  if command -v shasum >/dev/null 2>&1; then
    shasum -a 256 -c "$checksum_file"
  elif command -v sha256sum >/dev/null 2>&1; then
    sha256sum -c "$checksum_file"
  else
    echo "Missing required command: shasum or sha256sum" >&2
    exit 1
  fi
}

os="$(uname -s)"
arch="$(uname -m)"

case "$os" in
  Darwin) ;;
  Linux) ;;
  *)
    echo "Agents Notifier install script supports macOS and Linux. Use install.ps1 on Windows." >&2
    exit 1
    ;;
esac

if [ "$os" = "Darwin" ]; then
  case "$arch" in
    arm64) target="aarch64-apple-darwin" ;;
    x86_64) target="x86_64-apple-darwin" ;;
    *)
      echo "Unsupported macOS architecture: $arch" >&2
      exit 1
      ;;
  esac
else
  case "$arch" in
    x86_64) target="x86_64-unknown-linux-gnu" ;;
    *)
      echo "Unsupported Linux architecture: $arch" >&2
      exit 1
      ;;
  esac
fi

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

(cd "$tmp_dir" && verify_checksum "${archive}" "${archive}.sha256")

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
