#!/usr/bin/env bash
# Build the native desktop apps only: Linux and Windows (amd64).
set -euo pipefail
root=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
cd "$root"
if [[ $# != 0 ]]; then
  printf 'Usage: ./build.sh (native Linux and Windows desktop apps only)\n' >&2
  exit 2
fi
if [[ $(uname -s) != Linux ]]; then
  printf 'Use build.ps1 on Windows. macOS is not a release target.\n' >&2
  exit 2
fi
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$root/.target}"
mkdir -p "$CARGO_TARGET_DIR"
CARGO_TARGET_DIR=$(cd "$CARGO_TARGET_DIR" && pwd)
export CARGO_TARGET_DIR
npm --prefix desktop ci
npm --prefix desktop run build

build_desktop() {
  local target=$1 out=$2
  shift 2
  cargo "$@" --manifest-path src-tauri/Cargo.toml --release --target "$target"
  local src="$CARGO_TARGET_DIR/$target/release/dasiwa-simple-video-compare"
  [[ "$target" == *windows* ]] && src="${src}.exe"
  # Replace atomically so a running Linux instance can keep its existing inode.
  cp -p "$src" "${out}.new"
  mv -f "${out}.new" "$out"
  printf 'Built %s\n' "$out"
}

build_desktop x86_64-unknown-linux-gnu dasiwa-simple-video-compare-linux-amd64 build
if command -v cargo-xwin >/dev/null 2>&1; then
  build_desktop x86_64-pc-windows-msvc dasiwa-simple-video-compare-windows-amd64.exe xwin build
else
  printf 'Windows build skipped: install cargo-xwin and its Windows Rust target, or use build.ps1 on Windows.\n' >&2
fi
