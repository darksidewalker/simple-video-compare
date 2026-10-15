# Build the native Windows desktop app; Linux hosts use build.sh.
param()
$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot
if ($args.Count -ne 0) { throw "Usage: .\build.ps1 (native desktop app only)" }
if ($env:OS -ne "Windows_NT") {
    & bash ./build.sh
    if ($LASTEXITCODE -ne 0) { throw "Desktop build failed" }
    exit 0
}
$env:CARGO_TARGET_DIR = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $PSScriptRoot ".target" }
$env:CARGO_TARGET_DIR = [IO.Path]::GetFullPath($env:CARGO_TARGET_DIR)
& npm --prefix desktop ci
if ($LASTEXITCODE -ne 0) { throw "npm ci failed" }
& npm --prefix desktop run build
if ($LASTEXITCODE -ne 0) { throw "React build failed" }
& cargo build --manifest-path src-tauri/Cargo.toml --release --target x86_64-pc-windows-msvc
if ($LASTEXITCODE -ne 0) { throw "Windows desktop build failed" }
$source = Join-Path $env:CARGO_TARGET_DIR "x86_64-pc-windows-msvc\release\dasiwa-simple-video-compare.exe"
$output = Join-Path $PSScriptRoot "dasiwa-simple-video-compare-windows-amd64.exe"
Copy-Item $source "$output.new" -Force
Move-Item "$output.new" $output -Force
Write-Host "Built $output"
