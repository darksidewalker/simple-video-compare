#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
libmpv=$(readlink -f "${LIBMPV_PATH:-/usr/lib/libmpv.so}")

if [[ ! -f "$libmpv" ]]; then
  printf 'libmpv was not found. Set LIBMPV_PATH to its shared-library path.\n' >&2
  exit 1
fi

build() {
  local output=$1
  local output_dir
  output_dir=$(dirname "$output")
  mkdir -p "$output_dir/lib"
  go build -ldflags='-linkmode external -extldflags "-Wl,-rpath,$ORIGIN/lib"' -o "$output" ./cmd/dasiwa-simple-video-compare
  install -m 0755 "$libmpv" "$output_dir/lib/$(basename "$libmpv")"
  ln -sfn "$(basename "$libmpv")" "$output_dir/lib/libmpv.so.2"
  ln -sfn "libmpv.so.2" "$output_dir/lib/libmpv.so"
}

cd "$root"
build "$root/dasiwa-simple-video-compare-linux-amd64"
build "$root/dist/dasiwa-simple-video-compare-linux-amd64"
printf 'Built standalone Linux bundles with libmpv beside each binary.\n'
