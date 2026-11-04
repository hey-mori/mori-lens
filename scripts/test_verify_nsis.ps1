#!/usr/bin/env pwsh
# Mori Lens, NSIS verifier regression tests.
#
# Tests that the NSIS verifier correctly handles various scenarios.
# Uses temporary files and fixtures to avoid requiring real installers.
#
# Usage:
#   .\scripts\test_verify_nsis.ps1

$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$Verifier = Join-Path $ScriptDir "verify-nsis.ps1"
$TempDir = Join-Path $env:TEMP "mori-lens-nsis-test-$(Get-Random)"
New-Item -ItemType Directory -Path $TempDir -Force | Out-Null

$TestsPassed = 0
$TestsFailed = 0

function Test-Case($Name, $ScriptBlock, $ShouldSucceed) {
    Write-Host ""
    Write-Host "Test: $Name"
    try {
        $result = & $ScriptBlock
        if ($LASTEXITCODE -eq $null) {
            $exitCode = 0
        } else {
            $exitCode = $LASTEXITCODE
        }
        if ($ShouldSucceed -and $exitCode -eq 0) {
            Write-Host "  PASS (expected success, got success)"
            $script:TestsPassed++
        } elseif (-not $ShouldSucceed -and $exitCode -ne 0) {
            Write-Host "  PASS (expected failure, got failure)"
            $script:TestsPassed++
        } else {
            Write-Host "  FAIL (expected $(if ($ShouldSucceed) {'success'} else {'failure'}), got $(if ($exitCode -eq 0) {'success'} else {'failure'}))"
            $script:TestsFailed++
        }
    } catch {
        if (-not $ShouldSucceed) {
            Write-Host "  PASS (expected failure, got exception: $($_.Exception.Message))"
            $script:TestsPassed++
        } else {
            Write-Host "  FAIL (expected success, got exception: $($_.Exception.Message))"
            $script:TestsFailed++
        }
    }
}

try {
    # ==========================================================================
    # Test 1: Missing installer -> fail
    # ==========================================================================
    Test-Case "Missing installer" {
        & $Verifier -ExePath "$TempDir\nonexistent.exe"
    } -ShouldSucceed $false

    # ==========================================================================
    # Test 2: Non-PE/corrupt installer -> fail
    # ==========================================================================
    Test-Case "Non-PE file" {
        $fakeExe = Join-Path $TempDir "fake.exe"
        Set-Content -Path $fakeExe -Value "This is not a PE file" -Encoding Byte
        & $Verifier -ExePath $fakeExe
    } -ShouldSucceed $false

    # ==========================================================================
    # Test 3: Incorrect expected version -> fail
    # ==========================================================================
    Test-Case "Incorrect expected version" {
        # Create a minimal valid PE with wrong version (> 1MB to pass size check)
        $peExe = Join-Path $TempDir "Mori Lens_9.9.9_x64-setup.exe"
        # Minimal PE: MZ header + PE signature + basic headers + padding to > 1MB
        $bytes = New-Object byte[] (1024 * 1024 + 2048)  # 1MB + 2KB
        # MZ magic
        $bytes[0] = 0x4D; $bytes[1] = 0x5A
        # PE offset at 0x3C
        $bytes[0x3C] = 0x80
        # PE signature at offset
        $bytes[0x80] = 0x50; $bytes[0x81] = 0x45; $bytes[0x82] = 0x00; $bytes[0x83] = 0x00
        # Machine type (x86 bootstrap, valid for NSIS)
        $bytes[0x84] = 0x4C; $bytes[0x85] = 0x01
        [System.IO.File]::WriteAllBytes($peExe, $bytes)
        & $Verifier -ExePath $peExe -ExpectedVersion "0.1.0"
    } -ShouldSucceed $false

    # ==========================================================================
    # Test 4: Valid PE with correct version, no payload -> pass with warning
    # ==========================================================================
    Test-Case "Valid installer without payload verification" {
        # Create a minimal valid PE (> 1MB with version in name)
        $peExe = Join-Path $TempDir "Mori Lens_0.1.0_x64-setup.exe"
        $bytes = New-Object byte[] (1024 * 1024 + 2048)  # 1MB + 2KB
        $bytes[0] = 0x4D; $bytes[1] = 0x5A
        $bytes[0x3C] = 0x80
        $bytes[0x80] = 0x50; $bytes[0x81] = 0x45; $bytes[0x82] = 0x00; $bytes[0x83] = 0x00
        $bytes[0x84] = 0x4C; $bytes[0x85] = 0x01  # x86 bootstrap (valid for NSIS)
        [System.IO.File]::WriteAllBytes($peExe, $bytes)
        & $Verifier -ExePath $peExe -ExpectedVersion "0.1.0"
    } -ShouldSucceed $true

    # ==========================================================================
    # Test 5: Valid installer + x64 payload -> pass architecture contract
    # ==========================================================================
    Test-Case "x86 installer bootstrap + x64 payload" {
        # Create installer with x86 bootstrap (> 1MB with version in name)
        $installer = Join-Path $TempDir "Mori Lens_0.1.0_x64-setup.exe"
        $bytes = New-Object byte[] (1024 * 1024 + 2048)  # 1MB + 2KB
        $bytes[0] = 0x4D; $bytes[1] = 0x5A
        $bytes[0x3C] = 0x80
        $bytes[0x80] = 0x50; $bytes[0x81] = 0x45; $bytes[0x82] = 0x00; $bytes[0x83] = 0x00
        $bytes[0x84] = 0x4C; $bytes[0x85] = 0x01  # x86 bootstrap
        [System.IO.File]::WriteAllBytes($installer, $bytes)

        # Create payload with x64
        $payload = Join-Path $TempDir "payload-x64.exe"
        $bytes = New-Object byte[] 2048
        $bytes[0] = 0x4D; $bytes[1] = 0x5A
        $bytes[0x3C] = 0x80
        $bytes[0x80] = 0x50; $bytes[0x81] = 0x45; $bytes[0x82] = 0x00; $bytes[0x83] = 0x00
        $bytes[0x84] = 0x64; $bytes[0x85] = 0x86  # x64 payload
        [System.IO.File]::WriteAllBytes($payload, $bytes)

        & $Verifier -ExePath $installer -PayloadExePath $payload -ExpectedVersion "0.1.0"
    } -ShouldSucceed $true

    # ==========================================================================
    # Test 6: Valid installer + x86 payload -> fail (payload must be x64)
    # ==========================================================================
    Test-Case "x86 installer bootstrap + x86 payload (should fail)" {
        # Create installer with x86 bootstrap (> 1MB with version in name)
        $installer = Join-Path $TempDir "Mori Lens_0.1.0_x64-setup.exe"
        $bytes = New-Object byte[] (1024 * 1024 + 2048)  # 1MB + 2KB
        $bytes[0] = 0x4D; $bytes[1] = 0x5A
        $bytes[0x3C] = 0x80
        $bytes[0x80] = 0x50; $bytes[0x81] = 0x45; $bytes[0x82] = 0x00; $bytes[0x83] = 0x00
        $bytes[0x84] = 0x4C; $bytes[0x85] = 0x01  # x86 bootstrap
        [System.IO.File]::WriteAllBytes($installer, $bytes)

        # Create payload with x86 (should fail)
        $payload = Join-Path $TempDir "payload-x86.exe"
        $bytes = New-Object byte[] 2048
        $bytes[0] = 0x4D; $bytes[1] = 0x5A
        $bytes[0x3C] = 0x80
        $bytes[0x80] = 0x50; $bytes[0x81] = 0x45; $bytes[0x82] = 0x00; $bytes[0x83] = 0x00
        $bytes[0x84] = 0x4C; $bytes[0x85] = 0x01  # x86 payload (wrong!)
        [System.IO.File]::WriteAllBytes($payload, $bytes)

        & $Verifier -ExePath $installer -PayloadExePath $payload -ExpectedVersion "0.1.0"
    } -ShouldSucceed $false

    # ==========================================================================
    # Test 7: Valid installer + x64 payload (bootstrap also x64) -> pass
    # ==========================================================================
    Test-Case "x64 installer bootstrap + x64 payload" {
        # Create installer with x64 bootstrap (> 1MB with version in name)
        $installer = Join-Path $TempDir "Mori Lens_0.1.0_x64-setup.exe"
        $bytes = New-Object byte[] (1024 * 1024 + 2048)  # 1MB + 2KB
        $bytes[0] = 0x4D; $bytes[1] = 0x5A
        $bytes[0x3C] = 0x80
        $bytes[0x80] = 0x50; $bytes[0x81] = 0x45; $bytes[0x82] = 0x00; $bytes[0x83] = 0x00
        $bytes[0x84] = 0x64; $bytes[0x85] = 0x86  # x64 bootstrap
        [System.IO.File]::WriteAllBytes($installer, $bytes)

        # Create payload with x64
        $payload = Join-Path $TempDir "payload-x64-v2.exe"
        $bytes = New-Object byte[] 2048
        $bytes[0] = 0x4D; $bytes[1] = 0x5A
        $bytes[0x3C] = 0x80
        $bytes[0x80] = 0x50; $bytes[0x81] = 0x45; $bytes[0x82] = 0x00; $bytes[0x83] = 0x00
        $bytes[0x84] = 0x64; $bytes[0x85] = 0x86  # x64 payload
        [System.IO.File]::WriteAllBytes($payload, $bytes)

        & $Verifier -ExePath $installer -PayloadExePath $payload -ExpectedVersion "0.1.0"
    } -ShouldSucceed $true

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
