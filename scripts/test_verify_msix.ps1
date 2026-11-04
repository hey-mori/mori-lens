#!/usr/bin/env pwsh
# Mori Lens, MSIX verifier regression tests.
#
# Tests that verify-msix.ps1 correctly handles various scenarios using
# temporary fixtures.  No installation, no Store credentials, no real
# binary needed.
#
# Test matrix:
#   1. Wrong manifest version          -> FAIL
#   2. Bad checksum                    -> FAIL
#   3. x86 lens.exe (wrong arch)       -> FAIL
#   4. Missing lens.exe                -> FAIL
#   5. Valid structural fixture        -> PASS (with placeholder warning)
#   6. Placeholder publisher           -> structural PASS + warning
#   7. Invalid package zip             -> FAIL
#   8. Missing .sha256 sidecar         -> FAIL
#
# Usage:
#   .\scripts\test_verify_msix.ps1

$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$Verifier = Join-Path $ScriptDir "verify-msix.ps1"
$TempDir = Join-Path $env:TEMP "mori-lens-msix-test-$(Get-Random)"
New-Item -ItemType Directory -Path $TempDir -Force | Out-Null

Add-Type -Assembly System.IO.Compression.FileSystem
Add-Type -Assembly System.IO.Compression

$TestsPassed = 0
$TestsFailed = 0

function Test-Case($Name, $ScriptBlock, $ShouldSucceed) {
    Write-Host ""
    Write-Host "Test: $Name"
    try {
        $null = & $ScriptBlock 2>&1
        if ($LASTEXITCODE -eq $null) {
            $exitCode = 0
        } else {
            $exitCode = $LASTEXITCODE
        }
        if ($ShouldSucceed -and $exitCode -eq 0) {
            Write-Host "  RESULT: PASS (expected success, got success)"
            $script:TestsPassed++
        } elseif (-not $ShouldSucceed -and $exitCode -ne 0) {
            Write-Host "  RESULT: PASS (expected failure, got failure)"
            $script:TestsPassed++
        } else {
            $expected = if ($ShouldSucceed) { "success" } else { "failure" }
            $actual = if ($exitCode -eq 0) { "success" } else { "failure" }
            Write-Host "  RESULT: FAIL (expected $expected, got $actual)"
            $script:TestsFailed++
        }
    } catch {
        if (-not $ShouldSucceed) {
            Write-Host "  RESULT: PASS (expected failure, got exception: $($_.Exception.Message))"
            $script:TestsPassed++
        } else {
            Write-Host "  RESULT: FAIL (expected success, got exception: $($_.Exception.Message))"
            $script:TestsFailed++
        }
    }
}

# ---------------------------------------------------------------------------
# Helper: create a minimal PE bytes array with given machine type.
# ---------------------------------------------------------------------------
function New-MinimalPE([uint16]$MachineType) {
    $bytes = New-Object byte[] 512
    # MZ magic
    $bytes[0] = 0x4D; $bytes[1] = 0x5A
    # PE header offset at 0x3C -> point to 0x80 (128)
    $bytes[0x3C] = 0x80
    # PE signature "PE\0\0" at offset 0x80
    $bytes[0x80] = 0x50; $bytes[0x81] = 0x45; $bytes[0x82] = 0x00; $bytes[0x83] = 0x00
    # Machine type at offset 0x84 (PE offset + 4)
    $machBytes = [BitConverter]::GetBytes($MachineType)
    $bytes[0x84] = $machBytes[0]; $bytes[0x85] = $machBytes[1]
    return $bytes
}

# ---------------------------------------------------------------------------
# Helper: build an MSIX (zip) fixture with given manifest version and
# lens.exe machine type.  Returns the path to the .msix file.
# ---------------------------------------------------------------------------
function New-MsixFixture {
    param(
        [string]$Name,
        [string]$Version = "0.1.0.0",
        [uint16]$MachineType = 0x8664,
        [bool]$IncludeLensExe = $true,
        [string]$Publisher = "CN=PLACEHOLDERPUBLISHERNAME",
        [string]$PublisherDisplayName = "PLACEHOLDERPUBLISHERDISPLAYNAME"
    )

    $staging = Join-Path $TempDir "staging-$Name"
    New-Item -ItemType Directory -Path $staging -Force | Out-Null

    # AppxManifest.xml
    $manifest = @"
<?xml version="1.0" encoding="UTF-8"?>
<Package xmlns="http://schemas.microsoft.com/appx/manifest/foundation/windows10"
  xmlns:uap="http://schemas.microsoft.com/appx/manifest/uap/windows10"
  xmlns:rescap="http://schemas.microsoft.com/appx/manifest/foundation/windows10/restrictedcapabilities"
  IgnorableNamespaces="uap rescap">
  <Identity Name="TestPublisher.MoriLens" Publisher="$Publisher"
    Version="$Version" ProcessorArchitecture="x64" />
  <Properties>
    <DisplayName>Mori Lens</DisplayName>
    <PublisherDisplayName>$PublisherDisplayName</PublisherDisplayName>
    <Description>Test fixture</Description>
    <Logo>Assets\StoreLogo.png</Logo>
  </Properties>
  <Dependencies>
    <TargetDeviceFamily Name="Windows.Desktop"
      MinVersion="10.0.17763.0" MaxVersionTested="10.0.22621.0" />
  </Dependencies>
  <Applications>
    <Application Id="MoriLens" Executable="lens.exe"
      EntryPoint="Windows.FullTrustApplication">
    </Application>
  </Applications>
  <Capabilities>
    <rescap:Capability Name="runFullTrust" />
  </Capabilities>
</Package>
"@
    $utf8NoBom = New-Object System.Text.UTF8Encoding $false
    [System.IO.File]::WriteAllText(
        (Join-Path $staging "AppxManifest.xml"),
        $manifest,
        $utf8NoBom
    )

    # lens.exe (or omit it)
    if ($IncludeLensExe) {
        $peBytes = New-MinimalPE -MachineType $MachineType
        [System.IO.File]::WriteAllBytes((Join-Path $staging "lens.exe"), $peBytes)
    }

    # Assets directory with a placeholder
    $assetsDir = Join-Path $staging "Assets"
    New-Item -ItemType Directory -Path $assetsDir -Force | Out-Null
    # Create a minimal 1x1 PNG (67 bytes)
    $pngBytes = [Convert]::FromBase64String(
        "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg=="
    )
    [System.IO.File]::WriteAllBytes((Join-Path $assetsDir "StoreLogo.png"), $pngBytes)

    # Create the zip/msix
    $msixPath = Join-Path $TempDir "$Name.msix"
    if (Test-Path $msixPath) { Remove-Item $msixPath -Force }
    [System.IO.Compression.ZipFile]::CreateFromDirectory($staging, $msixPath)

    # Cleanup staging
    Remove-Item $staging -Recurse -Force

    return $msixPath
}

# ---------------------------------------------------------------------------
# Helper: create a .sha256 sidecar with either the correct or wrong hash.
# ---------------------------------------------------------------------------
function New-Sha256Sidecar {
    param(
        [string]$MsixPath,
        [bool]$Correct = $true
    )

    $shaPath = "$MsixPath.sha256"
    $msixName = [System.IO.Path]::GetFileName($MsixPath)

    if ($Correct) {
        $hash = (Get-FileHash $MsixPath -Algorithm SHA256).Hash
    } else {
        # Deliberately wrong hash
        $hash = "0000000000000000000000000000000000000000000000000000000000000000"
    }

    "$hash  $msixName" | Out-File $shaPath -Encoding ascii
    return $shaPath
}

try {
    Write-Host "Mori Lens MSIX Verifier Regression Tests"
    Write-Host "  Temp dir: $TempDir"
    Write-Host ""

    # ======================================================================
    # Test 1: Wrong manifest version -> FAIL
    # ======================================================================
    Test-Case "Wrong manifest version" {
        $msix = New-MsixFixture -Name "wrong-version" -Version "0.0.9.0"
        $null = New-Sha256Sidecar -MsixPath $msix -Correct $true
        & $Verifier -MsixPath $msix -ExpectedVersion "0.1.0.0"
    } -ShouldSucceed $false

    # ======================================================================
    # Test 2: Bad checksum -> FAIL
    # ======================================================================
    Test-Case "Bad checksum" {
        $msix = New-MsixFixture -Name "bad-checksum" -Version "0.1.0.0"
        $null = New-Sha256Sidecar -MsixPath $msix -Correct $false
        & $Verifier -MsixPath $msix -ExpectedVersion "0.1.0.0"
    } -ShouldSucceed $false

    # ======================================================================
    # Test 3: x86 lens.exe (wrong architecture) -> FAIL
    # ======================================================================
    Test-Case "x86 lens.exe" {
        $msix = New-MsixFixture -Name "x86-exe" -Version "0.1.0.0" -MachineType 0x014c
        $null = New-Sha256Sidecar -MsixPath $msix -Correct $true
        & $Verifier -MsixPath $msix -ExpectedVersion "0.1.0.0"
    } -ShouldSucceed $false

    # ======================================================================
    # Test 4: Missing lens.exe -> FAIL
    # ======================================================================
    Test-Case "Missing lens.exe" {
        $msix = New-MsixFixture -Name "no-exe" -Version "0.1.0.0" -IncludeLensExe $false
        $null = New-Sha256Sidecar -MsixPath $msix -Correct $true
        & $Verifier -MsixPath $msix -ExpectedVersion "0.1.0.0"
    } -ShouldSucceed $false

    # ======================================================================
    # Test 5: Valid structural fixture with placeholder publisher -> PASS
    # (placeholder publisher is a warning, not a failure)
    # ======================================================================
    Test-Case "Valid fixture with placeholder publisher" {
        $msix = New-MsixFixture -Name "valid-placeholder" -Version "0.1.0.0"
        $null = New-Sha256Sidecar -MsixPath $msix -Correct $true
        & $Verifier -MsixPath $msix -ExpectedVersion "0.1.0.0"
    } -ShouldSucceed $true

    # ======================================================================
    # Test 6: Valid fixture with real publisher -> PASS (no warnings)
    # ======================================================================
    Test-Case "Valid fixture with real publisher" {
        $msix = New-MsixFixture -Name "valid-real" -Version "0.1.0.0" `
            -Publisher "CN=RealPublisher" -PublisherDisplayName "Real Publisher Inc"
        $null = New-Sha256Sidecar -MsixPath $msix -Correct $true
        & $Verifier -MsixPath $msix -ExpectedVersion "0.1.0.0"
    } -ShouldSucceed $true

    # ======================================================================
    # Test 7: Invalid package zip -> FAIL
    # ======================================================================
    Test-Case "Invalid package zip" {
        $fakeMsix = Join-Path $TempDir "invalid.msix"
        [System.IO.File]::WriteAllBytes($fakeMsix, [System.Text.Encoding]::ASCII.GetBytes("This is not a zip file"))
        $fakeSha = "$fakeMsix.sha256"
        "0000000000000000000000000000000000000000000000000000000000000000  invalid.msix" | Out-File $fakeSha -Encoding ascii
        & $Verifier -MsixPath $fakeMsix -ExpectedVersion "0.1.0.0"
    } -ShouldSucceed $false

    # ======================================================================
    # Test 8: Missing .sha256 sidecar -> FAIL
    # ======================================================================
    Test-Case "Missing .sha256 sidecar" {
        $msix = New-MsixFixture -Name "no-sha256" -Version "0.1.0.0"
        # Deliberately do NOT create a .sha256 sidecar
        if (Test-Path "$msix.sha256") { Remove-Item "$msix.sha256" -Force }
        & $Verifier -MsixPath $msix -ExpectedVersion "0.1.0.0"
    } -ShouldSucceed $false

    # ======================================================================
    # Test 9: Well-formed old version must not pass
    # ======================================================================
    Test-Case "Well-formed old version rejected" {
        $msix = New-MsixFixture -Name "old-version" -Version "0.0.1.0"
        $null = New-Sha256Sidecar -MsixPath $msix -Correct $true
        & $Verifier -MsixPath $msix -ExpectedVersion "0.1.0.0"
    } -ShouldSucceed $false

} finally {
    # Cleanup
    Remove-Item $TempDir -Recurse -Force -ErrorAction SilentlyContinue
}

Write-Host ""
Write-Host "========================================="
Write-Host "Test Results: $TestsPassed passed, $TestsFailed failed"
Write-Host "========================================="

if ($TestsFailed -gt 0) {
    Write-Host "FAILED"
    exit 1
} else {
    Write-Host "ALL TESTS PASSED"
    exit 0
}
