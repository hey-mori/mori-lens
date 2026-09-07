# Mori Lens — MSIX packaging script.
#
# Builds the Windows release binary and packages it as an MSIX for
# Microsoft Store submission (ADR-0017).
#
# Prerequisites:
#   - Windows SDK (for MakeAppx.exe)
#   - Release binary built (cargo build --release)
#
# Usage:
#   .\scripts\package-msix.ps1
#
# Output:
#   target\msix\MoriLens_<version>_x64.msix

$ErrorActionPreference = "Stop"

$ProjectRoot = Split-Path -Parent $PSScriptRoot
$ManifestTemplate = Join-Path $ProjectRoot "packaging\windows-msix\AppxManifest.xml.in"
$AssetsDir = Join-Path $ProjectRoot "packaging\windows-msix\Assets"
$OutputDir = Join-Path $ProjectRoot "target\msix"
$StagingDir = Join-Path $OutputDir "staging"

# Read version from Cargo.toml
$CargoToml = Get-Content (Join-Path $ProjectRoot "Cargo.toml") -Raw
if ($CargoToml -match 'version\s*=\s*"(\d+\.\d+\.\d+)"') {
    $Version = $Matches[1]
} else {
    Write-Error "Could not read version from Cargo.toml"
    exit 1
}
$MsixVersion = "$Version.0"  # MSIX uses 4-part version

Write-Host "Mori Lens MSIX Packaging"
Write-Host "  Version: $MsixVersion"
Write-Host ""

# Check for MakeAppx.exe
$MakeAppx = Get-Command MakeAppx.exe -ErrorAction SilentlyContinue
if (-not $MakeAppx) {
    # Try Windows SDK default location
    $SdkPath = "C:\Program Files (x86)\Windows Kits\10\bin\*\x64\MakeAppx.exe"
    $Found = Get-Item $SdkPath -ErrorAction SilentlyContinue | Sort-Object FullName -Descending | Select-Object -First 1
    if ($Found) {
        $MakeAppx = $Found.FullName
    } else {
        Write-Error "MakeAppx.exe not found. Install Windows SDK."
        exit 1
    }
}

# Check release binary
$Binary = Join-Path $ProjectRoot "apps\lens\target\release\lens.exe"
if (-not (Test-Path $Binary)) {
    Write-Host "Release binary not found. Building..."
    Push-Location $ProjectRoot
    cargo build --release --manifest-path apps/lens/Cargo.toml
    Pop-Location
}

if (-not (Test-Path $Binary)) {
    Write-Error "Release binary not found at $Binary"
    exit 1
}

# Prepare staging directory
if (Test-Path $StagingDir) {
    Remove-Item $StagingDir -Recurse -Force
}
New-Item -ItemType Directory -Path $StagingDir -Force | Out-Null

# Copy binary
Copy-Item $Binary (Join-Path $StagingDir "lens.exe")

# Process manifest template — read as UTF-8 explicitly
$Manifest = [System.IO.File]::ReadAllText($ManifestTemplate, [System.Text.Encoding]::UTF8)
# Only replace the Version in the Identity element, not the XML declaration
$Manifest = $Manifest -replace '(<Identity[^>]*Version=")[^"]*(")', "`${1}$MsixVersion`$2"
# Note: Publisher placeholders remain — they must be replaced before Store submission
# Write manifest with UTF-8 no-BOM encoding (PowerShell 5.x Out-File adds BOM)
$Utf8NoBom = New-Object System.Text.UTF8Encoding $false
[System.IO.File]::WriteAllText(
    (Join-Path $StagingDir "AppxManifest.xml"),
    $Manifest,
    $Utf8NoBom
)

# Copy assets
$StagingAssets = Join-Path $StagingDir "Assets"
New-Item -ItemType Directory -Path $StagingAssets -Force | Out-Null
if (Test-Path $AssetsDir) {
    Copy-Item (Join-Path $AssetsDir "*") $StagingAssets -Force -ErrorAction SilentlyContinue
}

# Create placeholder assets if none exist
$RequiredAssets = @("StoreLogo.png", "Square44x44Logo.png", "Square150x150Logo.png", "Wide310x150Logo.png")
foreach ($Asset in $RequiredAssets) {
    $AssetPath = Join-Path $StagingAssets $Asset
    if (-not (Test-Path $AssetPath)) {
        Write-Host "  Warning: $Asset not found — Store submission will need real assets"
    }
}

# Package MSIX
$MsixName = "MoriLens_${MsixVersion}_x64.msix"
$MsixPath = Join-Path $OutputDir $MsixName

Write-Host ""
Write-Host "Packaging MSIX..."
Write-Host "  Input: $StagingDir"
Write-Host "  Output: $MsixPath"

& $MakeAppx pack /d $StagingDir /p $MsixPath /o

if ($LASTEXITCODE -ne 0) {
    Write-Error "MakeAppx failed with exit code $LASTEXITCODE"
    exit 1
}

# Cleanup staging
Remove-Item $StagingDir -Recurse -Force

# Generate SHA-256
$Hash = (Get-FileHash $MsixPath -Algorithm SHA256).Hash
$HashPath = "$MsixPath.sha256"
"$Hash  $MsixName" | Out-File $HashPath -Encoding ascii

Write-Host ""
Write-Host "MSIX packaged successfully:"
Write-Host "  $MsixPath"
Write-Host "  SHA-256: $Hash"
Write-Host ""
Write-Host "NOTE: This package uses PLACEHOLDER publisher identity."
Write-Host "      Replace placeholders in AppxManifest.xml.in before Store submission."
