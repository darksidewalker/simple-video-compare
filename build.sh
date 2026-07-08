#!/bin/bash
set -euo pipefail

echo "Building DaSiWa Simple Video Compare..."
cd "$(dirname "$0")"

# Function to build for specific platform
build_for() {
    local target_os=$1
    local target_arch=$2
    local output_name="dasiwa-simple-video-compare-${target_os}-${target_arch}"
    
    echo "Building for ${target_os}/${target_arch}..."
    
    export GOOS=${target_os}
    export GOARCH=${target_arch}
    
    # Add CGO_ENABLED=0 for static binaries on Linux
    if [[ "${target_os}" == "linux" ]]; then
        export CGO_ENABLED=0
    fi
    
    go build -ldflags="-s -w" -o "${output_name}" ./cmd/dasiwa-simple-video-compare/
    
    echo "✓ Created: ${output_name}"
    ls -lh "${output_name}"
    echo ""
}

# Build for all target platforms
echo "=== Building for multiple platforms ==="
echo ""

# Linux builds
build_for "linux" "amd64"
build_for "linux" "arm64"

# macOS builds
build_for "darwin" "amd64"
build_for "darwin" "arm64"

# Windows build
build_for "windows" "amd64"

echo "=== Build Summary ==="
echo "Generated binaries:"
ls -lh dasiwa-simple-video-compare-* 2>/dev/null | awk '{print $9, "(" $5 ")"}'

echo ""
echo "Usage:"
echo "  Linux:   ./dasiwa-simple-video-compare-linux-amd64"
echo "  macOS:   ./dasiwa-simple-video-compare-darwin-arm64"
echo "  Windows: dasiwa-simple-video-compare-windows-amd64.exe"
echo ""
echo "Options:"
echo "  --host HOST     Server host (default: 127.0.0.1)"
echo "  --port PORT     Server port (default: 8765)"
echo "  --no-open       Don't open browser automatically"
echo "  --browser       Open in normal browser instead of app window"
