# Mori Lens — MSIX verification script.
#
# Validates an MSIX package for Store submission readiness.
# Verifies structural correctness, version accuracy, payload
# architecture, and checksum integrity.
#
# Verification contract:
#   1. Structural: valid zip, AppxManifest.xml present, lens.exe present
#   2. Version: manifest Identity.Version must match Cargo.toml project
#      version in 4-part form (e.g. project 0.1.0 -> manifest 0.1.0.0)
#   3. Checksum: SHA-256 of the .msix must match the .sha256 sidecar
#      file that package-msix.ps1 always generates
#   4. Architecture: lens.exe PE machine type must be 0x8664 (AMD64/x64)
#   5. Signature: classified as SIGNED VALID, UNSIGNED, INVALID, or
#      UNKNOWN. Placeholder publisher is reported separately from
#      signature state.
#
# Usage:
#   .\scripts\verify-msix.ps1 <path-to-msix> [-ExpectedVersion <version>]
#
# If -ExpectedVersion is not provided, the version is read from
# apps/lens/Cargo.toml and converted to 4-part MSIX form.

param(
    [Parameter(Mandatory=$true)]
    [string]$MsixPath,

    [Parameter(Mandatory=$false)]
    [string]$ExpectedVersion
)

$ErrorActionPreference = "Stop"

if (-not (Test-Path $MsixPath)) {
    Write-Error "MSIX not found: $MsixPath"
    exit 1
}

# Derive expected version from Cargo.toml if not provided.
# The project version is semver (X.Y.Z); the MSIX manifest uses 4-part
# (X.Y.Z.0).  We compare in 4-part form.
if (-not $ExpectedVersion) {
    $CargoToml = Join-Path $PSScriptRoot "..\apps\lens\Cargo.toml"
    if (Test-Path $CargoToml) {
        $CargoContent = Get-Content $CargoToml -Raw
        if ($CargoContent -match 'version\s*=\s*"(\d+\.\d+\.\d+)"') {
            $ExpectedVersion = "$($Matches[1]).0"
        }
    }
}

Write-Host "Mori Lens MSIX Verification"
Write-Host "  Package: $MsixPath"
if ($ExpectedVersion) {
    Write-Host "  Expected version: $ExpectedVersion"
}
Write-Host ""

$Passed = 0
$Failed = 0
$Warnings = 0

function Check($Name, $Condition, $Message) {
    if ($Condition) {
        Write-Host "  PASS: $Name"
        $script:Passed++
    } else {
        Write-Host "  FAIL: $Name -- $Message"
        $script:Failed++
    }
}

function Warn($Name, $Message) {
    Write-Host "  WARN: $Name -- $Message"
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
    try {
        [System.IO.Compression.ZipFile]::ExtractToDirectory($MsixPath, $TempDir)
    } catch {
        Check "MSIX is valid zip" $false "Cannot extract: $($_.Exception.Message)"
        # Cannot proceed without extraction
        Write-Host ""
        Write-Host "Results: $Passed passed, $Failed failed, $Warnings warnings"
        Write-Host ""
        Write-Host "STATUS: NOT READY for Store submission"
        exit 1
    }
    Check "MSIX is valid zip" $true ""

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
            $ManifestVersion = $Identity.Version
            $IsFourPart = $ManifestVersion -match '^\d+\.\d+\.\d+\.\d+$'
            Check "Version format" $IsFourPart "Expected 4-part version, got: $ManifestVersion"

            # 7. Version matches expected
            if ($ExpectedVersion) {
                Check "Version matches project" ($ManifestVersion -eq $ExpectedVersion) "Expected $ExpectedVersion, got $ManifestVersion"
            }

            # 8. Publisher is not placeholder
            $IsPlaceholder = $Identity.Publisher -match 'PLACEHOLDER'
            if ($IsPlaceholder) {
                Warn "Publisher identity" "Still using PLACEHOLDER -- must be replaced before Store submission"
            }

            # 9. PublisherDisplayName is not placeholder
            $PubDisplay = $Manifest.Package.Properties.PublisherDisplayName
            $IsPubPlaceholder = $PubDisplay -match 'PLACEHOLDER'
            if ($IsPubPlaceholder) {
                Warn "PublisherDisplayName" "Still using PLACEHOLDER"
            }
        }

        # 10. Application element exists
        $Apps = $Manifest.Package.Applications.Application
        Check "Application element" ($null -ne $Apps) "Missing Application"

        # 11. Executable is lens.exe
        if ($Apps) {
            Check "Executable" ($Apps.Executable -eq "lens.exe") "Expected lens.exe, got: $($Apps.Executable)"
        }

        # 12. Target device family is Desktop
        $TargetFamily = $Manifest.Package.Dependencies.TargetDeviceFamily
        Check "Target device family" ($TargetFamily.Name -eq "Windows.Desktop") "Expected Windows.Desktop"
    }

    # 13. lens.exe exists in package and is x64
    $ExePath = Join-Path $TempDir "lens.exe"
    Check "lens.exe in package" (Test-Path $ExePath) "Missing lens.exe"

    if (Test-Path $ExePath) {
        # Read PE header to verify architecture.
        # MZ header: bytes[0..1] = 0x4D 0x5A
        # PE offset: bytes[0x3C..0x3F] (int32 LE)
        # PE signature: "PE\0\0" at PE offset
        # Machine type: uint16 at PE offset + 4
        #   0x8664 = AMD64 (x64)
        #   0x014c = i386 (x86)
        #   0xAA64 = ARM64
        $ExeBytes = [System.IO.File]::ReadAllBytes($ExePath)
        $HasMZ = ($ExeBytes.Length -ge 2 -and $ExeBytes[0] -eq 0x4D -and $ExeBytes[1] -eq 0x5A)
        Check "lens.exe PE header (MZ magic)" $HasMZ "Not a valid PE file"

        if ($HasMZ -and $ExeBytes.Length -ge 64) {
            $PeOffset = [BitConverter]::ToInt32($ExeBytes, 0x3C)
            if ($PeOffset -gt 0 -and ($PeOffset + 6) -le $ExeBytes.Length) {
                $PeSig = [System.Text.Encoding]::ASCII.GetString($ExeBytes, $PeOffset, 4)
                if ($PeSig -eq "PE`0`0") {
                    $MachineType = [BitConverter]::ToUInt16($ExeBytes, $PeOffset + 4)
                    $MachineName = switch ($MachineType) {
                        0x8664 { "x64 (AMD64)" }
                        0x014c { "x86 (i386)" }
                        0xAA64 { "ARM64" }
                        default { "Unknown (0x$($MachineType.ToString('X4')))" }
                    }
                    Check "lens.exe architecture (x64 required)" ($MachineType -eq 0x8664) "Expected x64 (0x8664), got $MachineName"
                } else {
                    Check "lens.exe PE signature" $false "PE signature not found at expected offset"
                }
            } else {
                Check "lens.exe PE offset" $false "Invalid or out of range"
            }
        }
    }

    # 14. Assets directory exists
    $AssetsPath = Join-Path $TempDir "Assets"
    if (Test-Path $AssetsPath) {
        $AssetFiles = Get-ChildItem $AssetsPath -File
        if ($AssetFiles.Count -eq 0) {
            Warn "Assets directory" "Empty -- Store submission needs real icon assets"
        } else {
            Check "Assets present" ($AssetFiles.Count -gt 0) "No assets"
        }
    } else {
        Warn "Assets directory" "Missing -- Store submission needs icon assets"
    }

} finally {
    Remove-Item $TempDir -Recurse -Force -ErrorAction SilentlyContinue
}

# 15. SHA-256 checksum verification.
# package-msix.ps1 always generates a .sha256 sidecar file.
# A missing checksum is a FAIL because the contract is that every
# packaged MSIX has a verified checksum.
$ShaPath = "$MsixPath.sha256"
if (Test-Path $ShaPath) {
    # The .sha256 file format from package-msix.ps1 is:
    #   "<HEXHASH>  <filename>"
    $ShaContent = (Get-Content $ShaPath -Raw).Trim()
    $ExpectedHash = ($ShaContent -split '\s+')[0].ToUpper()

    if ($ExpectedHash -match '^[0-9A-F]{64}$') {
        $ActualHash = (Get-FileHash $MsixPath -Algorithm SHA256).Hash.ToUpper()
        Check "SHA-256 checksum" ($ActualHash -eq $ExpectedHash) "Expected $ExpectedHash, got $ActualHash"
    } else {
        Check "SHA-256 checksum" $false "Malformed .sha256 file: could not parse expected hash"
    }
} else {
    Check "SHA-256 checksum" $false "Missing .sha256 sidecar (package-msix.ps1 always generates one)"
}

# 16. Signature state classification.
# Categories:
#   SIGNED VALID  - Authenticode signature is valid
#   UNSIGNED      - No Authenticode signature present
#   INVALID       - Signature present but invalid
#   UNKNOWN       - Could not determine (e.g. tool unavailable)
# Note: "placeholder publisher" is a separate concept from signature
# state.  A package can be unsigned with a real publisher, or signed
# with a placeholder.  These are reported independently.
$SigState = "UNKNOWN"
try {
    $Sig = Get-AuthenticodeSignature -FilePath $MsixPath -ErrorAction SilentlyContinue
    if ($null -ne $Sig) {
        $SigState = switch ($Sig.Status) {
            "Valid"     { "SIGNED VALID" }
            "NotSigned" { "UNSIGNED" }
            default     { "INVALID ($($Sig.Status))" }
        }
    }
} catch {
    $SigState = "UNKNOWN"
}

Write-Host ""
Write-Host "Signature state: $SigState"

if ($SigState -eq "UNSIGNED") {
    Warn "Authenticode signature" "Unsigned -- expected for development builds"
} elseif ($SigState -eq "SIGNED VALID") {
    Check "Authenticode signature" $true "Signed and valid"
} elseif ($SigState -like "INVALID*") {
    Warn "Authenticode signature" $SigState
} else {
    Warn "Authenticode signature" "State could not be determined"
}

# Summary
Write-Host ""
Write-Host "Results: $Passed passed, $Failed failed, $Warnings warnings"
Write-Host ""

if ($Failed -gt 0) {
    Write-Host "STATUS: NOT READY for Store submission"
    exit 1
} elseif ($Warnings -gt 0) {
    Write-Host "STATUS: STRUCTURALLY VALID -- placeholder values or unsigned; needs replacement before Store submission"
    exit 0
} else {
    Write-Host "STATUS: READY for Store submission"
    exit 0
}
