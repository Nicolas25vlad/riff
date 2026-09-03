#!/usr/bin/env bash
set -euo pipefail

REPO="https://github.com/Nicolas25vlad/riff"
CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
BIN_DIR="$CARGO_HOME/bin"

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

command -v curl >/dev/null 2>&1 || fail "curl is required"
command -v git >/dev/null 2>&1 || fail "git is required"

install_linux_audio_deps() {
  [ "$(uname -s)" = "Linux" ] || return 0

  if command -v pkg-config >/dev/null 2>&1 && pkg-config --exists alsa; then
    return 0
  fi

  say "Installing Linux audio build dependencies..."

  if command -v pacman >/dev/null 2>&1; then
    sudo pacman -S --needed --noconfirm base-devel alsa-lib pkgconf
  elif command -v apt-get >/dev/null 2>&1; then
    sudo apt-get update
    sudo apt-get install -y build-essential libasound2-dev pkg-config
  elif command -v dnf >/dev/null 2>&1; then
    sudo dnf install -y gcc make alsa-lib-devel pkgconf-pkg-config
  else
    fail "Riff needs an ALSA development package and pkg-config to build its Linux audio backend. Install them with your distro package manager and run this installer again."
  fi
}

if ! command -v cargo >/dev/null 2>&1; then
  if command -v rustup >/dev/null 2>&1; then
    say "Rustup found, installing the stable toolchain..."
    rustup default stable
  else
    fail "Cargo was not found. Install Rust first, then run this installer again."
  fi
fi

install_linux_audio_deps

say "Installing Riff from GitHub..."
cargo install --git "$REPO" --force

if ! command -v riff >/dev/null 2>&1; then
  warn "Riff was installed to $BIN_DIR, but that directory is not currently in PATH."

  case "${SHELL:-}" in
    */fish)
      warn "For Fish, run: fish_add_path $BIN_DIR"
      ;;
    *)
      warn "Add this to your shell PATH: export PATH=\"$BIN_DIR:\$PATH\""
      ;;
  esac
else
  say "Riff installed successfully."
  riff --version
fi

printf '\nTry it:\n  riff doctor\n  riff player\n  riff init\n  riff validate playlist.riff\n'
