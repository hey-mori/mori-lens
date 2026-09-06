# Mori Lens — MSIX verification script.
#
# Validates an MSIX package for Store submission readiness.
#
# Usage:
#   .\scripts\verify-msix.ps1 <path-to-msix>

param(
    [Parameter(Mandatory=$true)]
    [string]$MsixPath
)

$ErrorActionPreference = "Stop"

if (-not (Test-Path $MsixPath)) {
    Write-Error "MSIX not found: $MsixPath"
    exit 1
}

Write-Host "Mori Lens MSIX Verification"
Write-Host "  Package: $MsixPath"
Write-Host ""

$Passed = 0
$Failed = 0
$Warnings = 0

function Check($Name, $Condition, $Message) {
    if ($Condition) {
        Write-Host "  PASS: $Name"
        $script:Passed++
    } else {
        Write-Host "  FAIL: $Name — $Message"
        $script:Failed++
    }
}

function Warn($Name, $Message) {
    Write-Host "  WARN: $Name — $Message"
    $script:Warnings++
}

# 1. File exists and is non-empty
$FileInfo = Get-Item $MsixPath
Check "File exists" ($FileInfo.Length -gt 0) "Empty file"

# 2. File extension is .msix
Check "File extension" ($MsixPath -match '\.msix$') "Expected .msix extension"

# 3. Extract and validate manifest
$TempDir = Join-Path $env:TEMP "mori-lens-msix-verify-$(Get-Random)"
New-Item -ItemType Directory -Path $TempDir -Force | Out-Null

try {
    # Extract MSIX (it's a zip)
    Add-Type -Assembly System.IO.Compression.FileSystem
    [System.IO.Compression.ZipFile]::ExtractToDirectory($MsixPath, $TempDir)

    # 4. AppxManifest.xml exists
    $ManifestPath = Join-Path $TempDir "AppxManifest.xml"
    Check "AppxManifest.xml exists" (Test-Path $ManifestPath) "Missing manifest"

    if (Test-Path $ManifestPath) {
        $Manifest = [xml](Get-Content $ManifestPath)
        $Ns = @{ appx = "http://schemas.microsoft.com/appx/manifest/foundation/windows10" }

        # 5. Identity element exists
        $Identity = $Manifest.Package.Identity
        Check "Identity element" ($null -ne $Identity) "Missing Identity"

        if ($Identity) {
            # 6. Version format (4-part)
            Check "Version format" ($Identity.Version -match '^\d+\.\d+\.\d+\.\d+$') "Expected 4-part version, got: $($Identity.Version)"

            # 7. Publisher is not placeholder
            $IsPlaceholder = $Identity.Publisher -match 'PLACEHOLDER'
            if ($IsPlaceholder) {
                Warn "Publisher identity" "Still using PLACEHOLDER — must be replaced before Store submission"
            } else {
                Check "Publisher identity" (-not $IsPlaceholder) "Publisher is placeholder"
            }

            # 8. PublisherDisplayName is not placeholder
            $PubDisplay = $Manifest.Package.Properties.PublisherDisplayName
            $IsPubPlaceholder = $PubDisplay -match 'PLACEHOLDER'
            if ($IsPubPlaceholder) {
                Warn "PublisherDisplayName" "Still using PLACEHOLDER"
            }
        }

        # 9. Application element exists
        $Apps = $Manifest.Package.Applications.Application
        Check "Application element" ($null -ne $Apps) "Missing Application"

        # 10. Executable is lens.exe
        if ($Apps) {
            Check "Executable" ($Apps.Executable -eq "lens.exe") "Expected lens.exe, got: $($Apps.Executable)"
        }

        # 11. Target device family is Desktop
        $TargetFamily = $Manifest.Package.Dependencies.TargetDeviceFamily
        Check "Target device family" ($TargetFamily.Name -eq "Windows.Desktop") "Expected Windows.Desktop"
    }

    # 12. lens.exe exists in package
    $ExePath = Join-Path $TempDir "lens.exe"
    Check "lens.exe in package" (Test-Path $ExePath) "Missing lens.exe"

    # 13. Assets directory exists
    $AssetsPath = Join-Path $TempDir "Assets"
    if (Test-Path $AssetsPath) {
        $AssetFiles = Get-ChildItem $AssetsPath -File
        if ($AssetFiles.Count -eq 0) {
            Warn "Assets directory" "Empty — Store submission needs real icon assets"
        } else {
            Check "Assets present" ($AssetFiles.Count -gt 0) "No assets"
        }
    } else {
        Warn "Assets directory" "Missing — Store submission needs icon assets"
    }

} finally {
    Remove-Item $TempDir -Recurse -Force -ErrorAction SilentlyContinue
}

# 14. SHA-256 checksum file exists
$ShaPath = "$MsixPath.sha256"
if (Test-Path $ShaPath) {
    Check "SHA-256 checksum" $true "Checksum present"
} else {
    Warn "SHA-256 checksum" "No .sha256 file found alongside MSIX"
}

# Summary
Write-Host ""
Write-Host "Results: $Passed passed, $Failed failed, $Warnings warnings"
Write-Host ""

if ($Failed -gt 0) {
    Write-Host "STATUS: NOT READY for Store submission"
    exit 1
} elseif ($Warnings -gt 0) {
    Write-Host "STATUS: STRUCTURALLY VALID — placeholder values need replacement before Store submission"
    exit 0
} else {
    Write-Host "STATUS: READY for Store submission"
    exit 0
}
