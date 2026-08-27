<#
.SYNOPSIS
    Build and install Mori Lens in one step.

.DESCRIPTION
    A thin wrapper: builds a release binary, then hands off to install.ps1.
    Run this from anywhere; it finds the repository root on its own.

    This is the one command a contributor pastes after cloning.

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File scripts/build-and-install.ps1

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File scripts/build-and-install.ps1 -DryRun
#>
[CmdletBinding()]
param(
    [string] $Prefix,
    [switch] $NoShortcut,
    [switch] $NoRegister,
    [switch] $DryRun
)

$ErrorActionPreference = 'Stop'

$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$installer = Join-Path $here 'install.ps1'

# install.ps1 now builds on its own when the binary is not next to it, so
# this wrapper does not need to invoke cargo itself. It exists so the command
# a contributor pastes is self-documenting.
& $installer @PSBoundParameters
