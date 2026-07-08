param(
    [switch]$AllPlatforms,
    [string[]]$Targets = @("windows"),
    [string[]]$Architectures = @("amd64")
)

Write-Host "Building DaSiWa Simple Video Compare..." -ForegroundColor Cyan
$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot

function Invoke-GoBuild {
    param(
        [string]$TargetOS,
        [string]$TargetArch
    )
    
    $outputName = "dasiwa-simple-video-compare-${TargetOS}-${TargetArch}"
    if ($TargetOS -eq "windows") {
        $outputName += ".exe"
    }
    
    Write-Host "Building for ${TargetOS}/${TargetArch}..." -ForegroundColor Yellow
    
    $env:GOOS = $TargetOS
    $env:GOARCH = $TargetArch
    
    # Disable CGO for better compatibility
    $env:CGO_ENABLED = 0
    
    try {
        & go build -ldflags="-s -w" -o $outputName ./cmd/dasiwa-simple-video-compare/
        
        if ($LASTEXITCODE -eq 0) {
            Write-Host "✓ Created: $outputName" -ForegroundColor Green
            Get-Item $outputName | Select-Object Name, @{N='Size';E={'{0:N2} MB' -f ($_.Length/1MB)}}
            Write-Host ""
        } else {
            Write-Host "✗ Failed to build for ${TargetOS}/${TargetArch}" -ForegroundColor Red
            return $false
        }
    } catch {
        Write-Host "✗ Error building for ${TargetOS}/${TargetArch}: $_" -ForegroundColor Red
        return $false
    }
    
    return $true
}

if ($AllPlatforms) {
    Write-Host "`n=== Building for ALL platforms ===" -ForegroundColor Magenta
    Write-Host ""
    
    $platforms = @(
        @("linux", "amd64"),
        @("darwin", "amd64"),
        @("windows", "amd64")
    )
    
    foreach ($platform in $platforms) {
        Invoke-GoBuild -TargetOS $platform[0] -TargetArch $platform[1]
    }
} else {
    foreach ($target in $Targets) {
        foreach ($arch in $Architectures) {
            Invoke-GoBuild -TargetOS $target -TargetArch $arch
        }
    }
}

Write-Host "=== Build Summary ===" -ForegroundColor Cyan
Write-Host "Generated binaries:" -ForegroundColor White
Get-ChildItem -Filter "dasiwa-simple-video-compare-*" | 
    Where-Object { $_.Extension -ne '.zip' -and $_.Extension -ne '.tar.gz' } |
    Select-Object Name, @{N='Size';E={'{0:N2} MB' -f ($_.Length/1MB)}} |
    Format-Table -AutoSize

Write-Host "`nUsage Examples:" -ForegroundColor Cyan
Write-Host "  Linux/Mac:    ./dasiwa-simple-video-compare-linux-amd64"
Write-Host "  Windows:      .\dasiwa-simple-video-compare-windows-amd64.exe"
Write-Host ""
Write-Host "Available options:" -ForegroundColor Yellow
Write-Host "  --host HOST     Server host (default: 127.0.0.1)"
Write-Host "  --port PORT     Server port (default: 8765)"
Write-Host "  --no-open       Don't open browser automatically"
Write-Host "  --browser       Open in normal browser instead of app window"
