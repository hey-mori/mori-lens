#!/usr/bin/env pwsh
# Verify the Authenticode signature on a Mori Lens binary.
#
# Usage:
#   scripts/verify-signature.ps1 <path-to-binary>
#
# Exit codes:
#   0  signed and valid
#   1  unsigned or signature invalid
#   2  file not found or argument error
#
# This is for use when a signed build becomes available. Until then, every
# binary is unsigned and this script exits 1, which is the correct answer.

param(
    [Parameter(Mandatory = $true, Position = 0)]
    [string]$Path
)

if (-not (Test-Path -LiteralPath $Path)) {
    Write-Host "not found: $Path"
    exit 2
}

$item = Get-Item -LiteralPath $Path
if ($item.Extension -notin '.exe', '.dll', '.msix', '.appx') {
    Write-Host "not a signable file type: $($item.Extension)"
    exit 2
}

$signature = Get-AuthenticodeSignature -FilePath $Path -ErrorAction Stop

switch ($signature.Status) {
    'Valid' {
        $subject = $signature.SignerCertificate.Subject
        $issuer = $signature.SignerCertificate.Issuer
        Write-Host "signed"
        Write-Host "  subject: $subject"
        Write-Host "  issuer:  $issuer"
        Write-Host "  status:  $($signature.Status)"
        if ($signature.TimeStamperCertificate) {
            Write-Host "  timestamped: yes"
        } else {
            Write-Host "  timestamped: no"
        }
        exit 0
    }
    'NotSigned' {
        Write-Host "unsigned: $Path"
        Write-Host "  no Authenticode signature found"
        exit 1
    }
    default {
        Write-Host "signature invalid: $Path"
        Write-Host "  status: $($signature.Status)"
        Write-Host "  message: $($signature.StatusMessage)"
        exit 1
    }
}
