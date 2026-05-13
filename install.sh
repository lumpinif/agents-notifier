#!/bin/sh
set -eu

REPO="${AGENTS_ROUTER_REPO:-lumpinif/agents-router}"
INSTALL_DIR="${AGENTS_ROUTER_INSTALL_DIR:-$HOME/.local/bin}"
VERSION="${AGENTS_ROUTER_VERSION:-latest}"

need() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Missing required command: $1" >&2
    exit 1
  fi
}

need curl
need tar

service_is_running() {
  case "$os" in
    Darwin)
      uid="$(id -u)"
      for service_target in "gui/${uid}/com.agents-router.service" "user/${uid}/com.agents-router.service"; do
        if output="$(launchctl print "$service_target" 2>/dev/null)"; then
          if printf '%s\n' "$output" | grep -Eq '(^|[[:space:]])pid = [1-9][0-9]*|state = running'; then
            return 0
          fi
        fi
      done
      return 1
      ;;
    Linux)
      command -v systemctl >/dev/null 2>&1 &&
        systemctl --user is-active --quiet agents-router.service
      ;;
    *)
      return 1
      ;;
  esac
}

legacy_service_is_running() {
  case "$os" in
    Darwin)
      uid="$(id -u)"
      for service_target in "gui/${uid}/com.agents-notifier.service" "user/${uid}/com.agents-notifier.service"; do
        if output="$(launchctl print "$service_target" 2>/dev/null)"; then
          if printf '%s\n' "$output" | grep -Eq '(^|[[:space:]])pid = [1-9][0-9]*|state = running'; then
            return 0
          fi
        fi
      done
      return 1
      ;;
    Linux)
      command -v systemctl >/dev/null 2>&1 &&
        systemctl --user is-active --quiet agents-notifier.service
      ;;
    *)
      return 1
      ;;
  esac
}

service_metadata_path() {
  case "$os" in
    Darwin)
      printf '%s\n' "$HOME/Library/Application Support/agents-router/service.json"
      ;;
    Linux)
      printf '%s\n' "$HOME/.local/state/agents-router/service.json"
      ;;
    *)
      return 1
      ;;
  esac
}

service_config_path() {
  metadata_path="$(service_metadata_path)"
  if [ -f "$metadata_path" ]; then
    config_path="$(sed -n 's/^[[:space:]]*"config_path"[[:space:]]*:[[:space:]]*"\(.*\)"[[:space:]]*,\{0,1\}[[:space:]]*$/\1/p' "$metadata_path" | sed -n '1p')"
    if [ -n "$config_path" ]; then
      printf '%s\n' "$config_path"
      return
    fi
    echo "Could not read config_path from service metadata: $metadata_path" >&2
    exit 1
  fi

  printf '%s\n' "$HOME/.config/agents-router/config.toml"
}

default_config_path() {
  printf '%s\n' "$HOME/.config/agents-router/config.toml"
}

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
    echo "Agents Router install script supports macOS and Linux. Use install.ps1 on Windows." >&2
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

archive="agents-router-${target}.tar.gz"
if [ "$VERSION" = "latest" ]; then
  base_url="https://github.com/${REPO}/releases/latest/download"
elif expr "$VERSION" : 'v[0-9][0-9]*\.[0-9][0-9]*\.[0-9][0-9]*$' >/dev/null; then
  base_url="https://github.com/${REPO}/releases/download/${VERSION}"
else
  echo "AGENTS_ROUTER_VERSION must be latest or a vX.Y.Z tag; got: $VERSION" >&2
  exit 1
fi
tmp_dir="$(mktemp -d)"
installed_tmp=""
restart_service_after_install=0
start_after_legacy_migration=0

if service_is_running; then
  restart_service_after_install=1
fi
if legacy_service_is_running; then
  start_after_legacy_migration=1
fi

cleanup() {
  rm -rf "$tmp_dir"
  if [ -n "$installed_tmp" ]; then
    rm -f "$installed_tmp"
  fi
}
trap cleanup EXIT INT TERM

echo "Downloading Agents Router for ${target}..."
curl -fsSL "${base_url}/${archive}" -o "${tmp_dir}/${archive}"
curl -fsSL "${base_url}/${archive}.sha256" -o "${tmp_dir}/${archive}.sha256"

(cd "$tmp_dir" && verify_checksum "${archive}" "${archive}.sha256")

tar -xzf "${tmp_dir}/${archive}" -C "$tmp_dir"
mkdir -p "$INSTALL_DIR"
installed_tmp="${INSTALL_DIR}/.agents-router.$$"
cp "${tmp_dir}/agents-router" "$installed_tmp"
chmod 0755 "$installed_tmp"
mv "$installed_tmp" "${INSTALL_DIR}/agents-router"
installed_tmp=""
printf '%s\n' "script" > "${INSTALL_DIR}/.agents-router-install-method"

echo "Installed: ${INSTALL_DIR}/agents-router"

"${INSTALL_DIR}/agents-router" migrate-legacy --config "$(default_config_path)"

if [ "$restart_service_after_install" = "1" ]; then
  echo "Restarting existing Agents Router service..."
  config_path="$(service_config_path)"
  "${INSTALL_DIR}/agents-router" stop
  "${INSTALL_DIR}/agents-router" start --config "$config_path"
elif [ "$start_after_legacy_migration" = "1" ]; then
  config_path="$(service_config_path)"
  if [ -f "$config_path" ]; then
    echo "Starting Agents Router with migrated config..."
    "${INSTALL_DIR}/agents-router" start --config "$config_path"
  fi
fi

case ":$PATH:" in
  *":$INSTALL_DIR:"*) ;;
  *)
    echo
    echo "Add this to your shell profile if agents-router is not found:"
    echo "  export PATH=\"$INSTALL_DIR:\$PATH\""
    ;;
esac

echo
if [ "$restart_service_after_install" = "1" ]; then
  echo "Service restarted with the installed version."
elif [ "$start_after_legacy_migration" = "1" ] && [ -f "$(service_config_path)" ]; then
  echo "Service started with the migrated Agents Router config."
else
  echo "Next:"
  echo "  agents-router setup"
fi
