#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

say() {
  printf '\033[1;36m%s\033[0m\n' "$1"
}

warn() {
  printf '\033[1;33m%s\033[0m\n' "$1"
}

fail() {
  printf '\033[1;31merror:\033[0m %s\n' "$1" >&2
  exit 1
}

install_native_deps() {
  case "$(uname -s)" in
    Linux)
      if command -v pkg-config >/dev/null 2>&1 \
        && pkg-config --exists alsa \
        && pkg-config --exists libpulse; then
        say "Native Linux/WSL audio dependencies are already installed."
        return 0
      fi

      say "Installing native Linux/WSL audio dependencies..."
      if command -v pacman >/dev/null 2>&1; then
        sudo pacman -S --needed --noconfirm base-devel alsa-lib libpulse pkgconf
      elif command -v apt-get >/dev/null 2>&1; then
        sudo apt-get update
        sudo apt-get install -y build-essential libasound2-dev libpulse-dev pkg-config
      elif command -v dnf >/dev/null 2>&1; then
        sudo dnf install -y gcc make alsa-lib-devel pulseaudio-libs-devel pkgconf-pkg-config
      else
        fail "Unsupported Linux package manager. Install ALSA and PulseAudio development packages, pkg-config and a C toolchain manually."
      fi
      ;;
    Darwin)
      say "macOS detected. No extra native package is required by this helper right now."
      ;;
    MINGW*|MSYS*|CYGWIN*)
      say "Windows shell detected. Native audio dependencies are provided by the Windows/Rodio build."
      ;;
    *)
      warn "Automatic native dependency installation is not supported on this OS yet."
      ;;
  esac
}

install_rust_deps() {
  command -v cargo >/dev/null 2>&1 || fail "Cargo is required. Install Rust with rustup first."
  say "Fetching Rust dependencies..."
  cargo fetch
  say "Dependencies are ready."
}

update_rust_deps() {
  command -v cargo >/dev/null 2>&1 || fail "Cargo is required. Install Rust with rustup first."
  say "Updating Rust dependencies within Cargo.toml constraints..."
  cargo update
  say "Dependency resolution updated. Review Cargo.lock before committing."
}

check_deps() {
  command -v cargo >/dev/null 2>&1 || fail "Cargo is required. Install Rust with rustup first."
  say "Checking dependency graph and project compilation..."
  cargo tree >/dev/null
  cargo check --all-targets --all-features
  say "Dependency check passed."
}

usage() {
  cat <<'EOF'
Riff dependency helper

Usage:
  bash scripts/deps.sh install   Install native dependencies and fetch Rust crates
  bash scripts/deps.sh update    Update Rust dependencies within Cargo.toml constraints
  bash scripts/deps.sh check     Validate dependency resolution and compile the project
  bash scripts/deps.sh native    Install only native/system dependencies
  bash scripts/deps.sh rust      Fetch only Rust dependencies
  bash scripts/deps.sh help      Show this help
EOF
}

case "${1:-install}" in
  install)
    install_native_deps
    install_rust_deps
    ;;
  update)
    update_rust_deps
    ;;
  check)
    check_deps
    ;;
  native)
    install_native_deps
    ;;
  rust)
    install_rust_deps
    ;;
  help|-h|--help)
    usage
    ;;
  *)
    usage >&2
    exit 2
    ;;
esac
