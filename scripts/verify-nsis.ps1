# Mori Lens — NSIS installer structural verification.
#
# Validates an NSIS installer for structural correctness.
# This is NOT an installation test — it verifies the artifact shape.
#
# Usage:
#   .\scripts\verify-nsis.ps1 <path-to-exe> [-ExpectedVersion <version>]
#
# If -ExpectedVersion is not provided, reads from apps/lens/Cargo.toml.

param(
    [Parameter(Mandatory=$true)]
    [string]$ExePath,

    [Parameter(Mandatory=$false)]
    [string]$ExpectedVersion
)

$ErrorActionPreference = "Stop"

if (-not (Test-Path $ExePath)) {
    Write-Error "NSIS installer not found: $ExePath"
    exit 1
}

# Read expected version from Cargo.toml if not provided
if (-not $ExpectedVersion) {
    $CargoToml = Join-Path $PSScriptRoot "..\apps\lens\Cargo.toml"
    if (Test-Path $CargoToml) {
        $ExpectedVersion = (Select-String -Path $CargoToml -Pattern '^version\s*=\s*"([^"]+)"' |
            Select-Object -First 1).Matches.Groups[1].Value
    }
}

Write-Host "Mori Lens NSIS Installer Verification"
Write-Host "  Installer: $ExePath"
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
        Write-Host "  FAIL: $Name — $Message"
        $script:Failed++
    }
}

function Warn($Name, $Message) {
    Write-Host "  WARN: $Name — $Message"
    $script:Warnings++
}

# 1. File exists and is non-empty
$FileInfo = Get-Item $ExePath
Check "File exists" ($FileInfo.Length -gt 0) "Empty file"

# 2. File extension is .exe
Check "File extension" ($ExePath -match '\.exe$') "Expected .exe extension"

# 3. Nontrivial size (>1MB)
$MinSize = 1MB
Check "Minimum size" ($FileInfo.Length -gt $MinSize) "Installer too small ($($FileInfo.Length) bytes < $MinSize)"

# 4. PE header readable (check MZ magic bytes)
$Bytes = [System.IO.File]::ReadAllBytes($ExePath)
$HasMZ = ($Bytes.Length -ge 2 -and $Bytes[0] -eq 0x4D -and $Bytes[1] -eq 0x5A)
Check "PE header (MZ magic)" $HasMZ "Not a valid PE file"

# 5. Version info via FileVersionInfo
$VersionInfo = $null
try {
    $VersionInfo = [System.Diagnostics.FileVersionInfo]::GetVersionInfo($ExePath)
} catch {
    # Version info extraction failed
}

if ($VersionInfo) {
    # Product name
    if ($VersionInfo.ProductName) {
        Check "ProductName" ($VersionInfo.ProductName -match 'Mori|Lens') "Unexpected: $($VersionInfo.ProductName)"
    } else {
        Warn "ProductName" "Not set in version info"
    }

    # File description
    if ($VersionInfo.FileDescription) {
        Check "FileDescription present" $true ""
    } else {
        Warn "FileDescription" "Not set in version info"
    }

    # Product version
    if ($VersionInfo.ProductVersion) {
        if ($ExpectedVersion) {
            $VersionMatch = $VersionInfo.ProductVersion -match [regex]::Escape($ExpectedVersion)
            Check "ProductVersion matches" $VersionMatch "Got $($VersionInfo.ProductVersion), expected $ExpectedVersion"
        } else {
            Check "ProductVersion present" $true ""
        }
    } else {
        Warn "ProductVersion" "Not set in version info"
    }

    # File version
    if ($VersionInfo.FileVersion) {
        if ($ExpectedVersion) {
            $FileVerMatch = $VersionInfo.FileVersion -match [regex]::Escape($ExpectedVersion)
            Check "FileVersion matches" $FileVerMatch "Got $($VersionInfo.FileVersion), expected $ExpectedVersion"
        } else {
            Check "FileVersion present" $true ""
        }
    } else {
        Warn "FileVersion" "Not set in version info"
    }
} else {
    Warn "VersionInfo" "Could not read version information"
}

# 6. Architecture detection (PE machine type)
if ($HasMZ -and $Bytes.Length -ge 64) {
    # PE header offset is at 0x3C
    $PeOffset = [BitConverter]::ToInt32($Bytes, 0x3C)
    if ($PeOffset -gt 0 -and ($PeOffset + 6) -lt $Bytes.Length) {
        # PE signature should be "PE\0\0"
        $PeSig = [System.Text.Encoding]::ASCII.GetString($Bytes, $PeOffset, 4)
        if ($PeSig -eq "PE`0`0") {
            $Machine = [BitConverter]::ToUInt16($Bytes, $PeOffset + 4)
            $MachineName = switch ($Machine) {
                0x8664 { "x64 (AMD64)" }
                0x014c { "x86 (i386)" }
                0xAA64 { "ARM64" }
                default { "Unknown (0x$($Machine.ToString('X4')))" }
            }
            Check "Architecture" ($Machine -eq 0x8664) "Expected x64, got $MachineName"
        } else {
            Warn "PE signature" "Not found at expected offset"
        }
    } else {
        Warn "PE offset" "Invalid or out of range"
    }
}

# 7. Filename version check
if ($ExpectedVersion) {
    $Filename = [System.IO.Path]::GetFileName($ExePath)
    $FilenameHasVersion = $Filename -match [regex]::Escape($ExpectedVersion)
    Check "Filename contains version" $FilenameHasVersion "$Filename does not contain $ExpectedVersion"
}

# 8. Authenticode signature check
$SigStatus = "UNKNOWN"
try {
    $Sig = Get-AuthenticodeSignature -FilePath $ExePath -ErrorAction SilentlyContinue
    if ($Sig) {
        $SigStatus = switch ($Sig.Status) {
            "Valid" { "SIGNED" }
            "NotSigned" { "UNSIGNED" }
            default { "INVALID ($($Sig.Status))" }
        }
    }
} catch {
    $SigStatus = "UNKNOWN (Get-AuthenticodeSignature failed)"
}

# Report signature state but don't fail on unsigned (signing is external blocker)
if ($SigStatus -eq "SIGNED") {
    Check "Authenticode signature" $true "Signed"
} elseif ($SigStatus -eq "UNSIGNED") {
    Warn "Authenticode signature" "Unsigned — expected for development builds (signing is external blocker)"
} else {
    Warn "Authenticode signature" $SigStatus
}

# Summary
Write-Host ""
Write-Host "Results: $Passed passed, $Failed failed, $Warnings warnings"
Write-Host ""

# Classification
Write-Host "STRUCTURE:"
if ($Failed -eq 0) {
    Write-Host "  PASS"
} else {
    Write-Host "  FAIL"
}

Write-Host ""
Write-Host "SIGNATURE:"
Write-Host "  $SigStatus"
Write-Host ""

if ($Failed -gt 0) {
    Write-Host "STATUS: STRUCTURAL VERIFICATION FAILED"
    exit 1
} else {
    Write-Host "STATUS: STRUCTURALLY VALID"
    exit 0
}
