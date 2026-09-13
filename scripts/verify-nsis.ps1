# Mori Lens — NSIS installer structural verification.
#
# Validates an NSIS installer for structural correctness.
# This is NOT an installation test — it verifies the artifact shape.
#
# NSIS installers have a two-layer architecture:
#   1. INSTALLER BOOTSTRAP: x86 PE for maximum Windows compatibility
#   2. APPLICATION PAYLOAD: the actual lens.exe (must be x64)
#
# This verifier checks BOTH layers independently.
#
# Usage:
#   .\scripts\verify-nsis.ps1 <path-to-exe> [-PayloadExePath <path>] [-ExpectedVersion <version>]
#
# If -PayloadExePath is provided, the application payload architecture is verified.
# If -ExpectedVersion is not provided, reads from apps/lens/Cargo.toml.

param(
    [Parameter(Mandatory=$true)]
    [string]$ExePath,

    [Parameter(Mandatory=$false)]
    [string]$PayloadExePath,

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
if ($PayloadExePath) {
    Write-Host "  Payload: $PayloadExePath"
}
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

# ============================================================================
# INSTALLER BOOTSTRAP CHECKS
# ============================================================================

# 1. File exists and is non-empty
$FileInfo = Get-Item $ExePath
Check "Installer exists" ($FileInfo.Length -gt 0) "Empty file"

# 2. File extension is .exe
Check "Installer extension" ($ExePath -match '\.exe$') "Expected .exe extension"

# 3. Nontrivial size (>1MB)
$MinSize = 1MB
Check "Installer minimum size" ($FileInfo.Length -gt $MinSize) "Installer too small ($($FileInfo.Length) bytes < $MinSize)"

# 4. PE header readable (check MZ magic bytes)
$Bytes = [System.IO.File]::ReadAllBytes($ExePath)
$HasMZ = ($Bytes.Length -ge 2 -and $Bytes[0] -eq 0x4D -and $Bytes[1] -eq 0x5A)
Check "Installer PE header (MZ magic)" $HasMZ "Not a valid PE file"

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
        Check "Installer ProductName" ($VersionInfo.ProductName -match 'Mori|Lens') "Unexpected: $($VersionInfo.ProductName)"
    } else {
        Warn "Installer ProductName" "Not set in version info"
    }

    # File description
    if ($VersionInfo.FileDescription) {
        Check "Installer FileDescription present" $true ""
    } else {
        Warn "Installer FileDescription" "Not set in version info"
    }

    # Product version
    if ($VersionInfo.ProductVersion) {
        if ($ExpectedVersion) {
            $VersionMatch = $VersionInfo.ProductVersion -match [regex]::Escape($ExpectedVersion)
            Check "Installer ProductVersion matches" $VersionMatch "Got $($VersionInfo.ProductVersion), expected $ExpectedVersion"
        } else {
            Check "Installer ProductVersion present" $true ""
        }
    } else {
        Warn "Installer ProductVersion" "Not set in version info"
    }

    # File version
    if ($VersionInfo.FileVersion) {
        if ($ExpectedVersion) {
            $FileVerMatch = $VersionInfo.FileVersion -match [regex]::Escape($ExpectedVersion)
            Check "Installer FileVersion matches" $FileVerMatch "Got $($VersionInfo.FileVersion), expected $ExpectedVersion"
        } else {
            Check "Installer FileVersion present" $true ""
        }
    } else {
        Warn "Installer FileVersion" "Not set in version info"
    }
} else {
    Warn "Installer VersionInfo" "Could not read version information"
}

# 6. Installer bootstrap architecture (PE machine type)
# NSIS installers legitimately use x86 bootstrap for Windows compatibility.
# This is NOT the application architecture — that's checked separately.
if ($HasMZ -and $Bytes.Length -ge 64) {
    # PE header offset is at 0x3C
    $PeOffset = [BitConverter]::ToInt32($Bytes, 0x3C)
    if ($PeOffset -gt 0 -and ($PeOffset + 6) -lt $Bytes.Length) {
        # PE signature should be "PE\0\0"
        $PeSig = [System.Text.Encoding]::ASCII.GetString($Bytes, $PeOffset, 4)
        if ($PeSig -eq "PE`0`0") {
            $BootstrapMachine = [BitConverter]::ToUInt16($Bytes, $PeOffset + 4)
            $BootstrapMachineName = switch ($BootstrapMachine) {
                0x8664 { "x64 (AMD64)" }
                0x014c { "x86 (i386)" }
                0xAA64 { "ARM64" }
                default { "Unknown (0x$($BootstrapMachine.ToString('X4')))" }
            }
            # NSIS bootstrap may be x86 or x64 — both are valid
            Check "Installer bootstrap architecture" ($BootstrapMachine -eq 0x8664 -or $BootstrapMachine -eq 0x014c) "Unexpected bootstrap architecture: $BootstrapMachineName"
        } else {
            Warn "Installer PE signature" "Not found at expected offset"
        }
    } else {
        Warn "Installer PE offset" "Invalid or out of range"
    }
}

# 7. Filename version check
if ($ExpectedVersion) {
    $Filename = [System.IO.Path]::GetFileName($ExePath)
    $FilenameHasVersion = $Filename -match [regex]::Escape($ExpectedVersion)
    Check "Installer filename contains version" $FilenameHasVersion "$Filename does not contain $ExpectedVersion"
}

# ============================================================================
# APPLICATION PAYLOAD CHECKS
# ============================================================================

if ($PayloadExePath) {
    Write-Host ""
    Write-Host "Application Payload Verification:"
    Write-Host ""

    if (-not (Test-Path $PayloadExePath)) {
        Check "Payload exists" $false "Payload not found: $PayloadExePath"
    } else {
        Check "Payload exists" $true ""

        $PayloadInfo = Get-Item $PayloadExePath
        Check "Payload non-empty" ($PayloadInfo.Length -gt 0) "Empty payload"

        # Read payload PE header
        $PayloadBytes = [System.IO.File]::ReadAllBytes($PayloadExePath)
        $PayloadHasMZ = ($PayloadBytes.Length -ge 2 -and $PayloadBytes[0] -eq 0x4D -and $PayloadBytes[1] -eq 0x5A)
        Check "Payload PE header (MZ magic)" $PayloadHasMZ "Not a valid PE file"

        if ($PayloadHasMZ -and $PayloadBytes.Length -ge 64) {
            $PayloadPeOffset = [BitConverter]::ToInt32($PayloadBytes, 0x3C)
            if ($PayloadPeOffset -gt 0 -and ($PayloadPeOffset + 6) -lt $PayloadBytes.Length) {
                $PayloadPeSig = [System.Text.Encoding]::ASCII.GetString($PayloadBytes, $PayloadPeOffset, 4)
                if ($PayloadPeSig -eq "PE`0`0") {
                    $PayloadMachine = [BitConverter]::ToUInt16($PayloadBytes, $PayloadPeOffset + 4)
                    $PayloadMachineName = switch ($PayloadMachine) {
                        0x8664 { "x64 (AMD64)" }
                        0x014c { "x86 (i386)" }
                        0xAA64 { "ARM64" }
                        default { "Unknown (0x$($PayloadMachine.ToString('X4')))" }
                    }
                    # CRITICAL: Payload MUST be x64 for current Windows target
                    Check "Payload architecture (x64 required)" ($PayloadMachine -eq 0x8664) "Expected x64, got $PayloadMachineName"
                } else {
                    Check "Payload PE signature" $false "Not found at expected offset"
                }
            } else {
                Check "Payload PE offset" $false "Invalid or out of range"
            }
        }

        # Payload version check
        $PayloadVersionInfo = $null
        try {
            $PayloadVersionInfo = [System.Diagnostics.FileVersionInfo]::GetVersionInfo($PayloadExePath)
        } catch {
            # Version info extraction failed
        }

        if ($PayloadVersionInfo -and $ExpectedVersion) {
            if ($PayloadVersionInfo.ProductVersion) {
                $PayloadVersionMatch = $PayloadVersionInfo.ProductVersion -match [regex]::Escape($ExpectedVersion)
                Check "Payload ProductVersion matches" $PayloadVersionMatch "Got $($PayloadVersionInfo.ProductVersion), expected $ExpectedVersion"
            } else {
                Warn "Payload ProductVersion" "Not set in version info"
            }
        }
    }
} else {
    # No payload path provided — warn that payload architecture is unverified
    Warn "Payload architecture" "Not verified (use -PayloadExePath to verify the bundled lens.exe)"
}

# ============================================================================
# SIGNATURE CHECK
# ============================================================================

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

# ============================================================================
# SUMMARY
# ============================================================================

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
