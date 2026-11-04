<#
.SYNOPSIS
    Remove Mori Lens for the current user.

.DESCRIPTION
    Reverses install.ps1: the Start Menu entry, the Add/Remove Programs entry,
    the start-with-Windows key if it is set, and the installed folder.

    **The settings file is kept**, and that is a decision rather than an
    oversight. It is the only thing in this product a person wrote themselves:
    their per-application rules, their target language, their endpoint. An
    uninstaller that deleted it would take work away from somebody who is
    probably reinstalling. `-Purge` removes it, and says so first.

    Per-user throughout, exactly as the installer is. Nothing outside this
    user's profile and HKCU is touched, so nothing here needs elevation.

.PARAMETER Prefix
    Where it was installed. Defaults to the location the installer recorded,
    then to %LOCALAPPDATA%\Programs\Mori Lens.

.PARAMETER Purge
    Also delete the settings file.

.PARAMETER DryRun
    Say what would happen and change nothing.
#>
[CmdletBinding()]
param(
    [string] $Prefix,
    [switch] $Purge,
    [switch] $DryRun
)

$ErrorActionPreference = 'Stop'

$AppName = 'Mori Lens'
$RegistryKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\MoriLens'
$RunKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
# The value name `autostart.rs` writes, which has a space in it. Not the
# uninstall key's name, which does not. They look interchangeable and are not:
# getting this wrong leaves a startup entry pointing at a deleted binary, and
# Windows reports that as a startup item that failed rather than one that was
# removed.
$RunValue = 'Mori Lens'
# The product's own registry namespace. Only the live suite writes under it,
# and a key left behind is the thing this step exists to prevent.
$ProductKey = 'HKCU:\Software\Mori Lens'

function Say($message) { Write-Host "  $message" }
function Step($message) { Write-Host "`n- $message" }

if (-not $Prefix) {
    # What the installer recorded, so a custom prefix is remembered rather
    # than guessed at. A missing key falls back to the default location.
    $recorded = $null
    try {
        $recorded = (Get-ItemProperty -Path $RegistryKey -Name 'InstallLocation' `
            -ErrorAction Stop).InstallLocation
    } catch {}
    if ($recorded) {
        $Prefix = $recorded
    } else {
        $Prefix = Join-Path $env:LOCALAPPDATA 'Programs\Mori Lens'
    }
}

Write-Host "`n$AppName uninstaller"
Say "prefix    $Prefix"
if ($DryRun) { Say 'DRY RUN, nothing will be changed' }

$running = Get-Process -Name 'lens' -ErrorAction SilentlyContinue
if ($running) {
    Write-Host "`nMori Lens is running. Quit it from the tray icon and run this again.`n"
    exit 1
}

Step 'start with Windows'
# Removed whether or not the tick was ever set. A run-key entry left behind
# points at a binary that is about to stop existing, and Windows reports that
# as a startup item that failed rather than as one that was uninstalled.
$startup = $null
try {
    $startup = (Get-ItemProperty -Path $RunKey -Name $RunValue -ErrorAction Stop).$RunValue
} catch {}
if ($startup) {
    if (-not $DryRun) {
        Remove-ItemProperty -Path $RunKey -Name $RunValue -ErrorAction SilentlyContinue
    }
    Say 'removed'
} else {
    Say 'was not set'
}

Step 'Start Menu'
$link = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs\$AppName.lnk"
if (Test-Path $link) {
    if (-not $DryRun) { Remove-Item -Path $link -Force }
    Say $link
} else {
    Say 'no shortcut'
}

Step 'Settings > Apps'
if (Test-Path $RegistryKey) {
    if (-not $DryRun) { Remove-Item -Path $RegistryKey -Recurse -Force }
    Say 'removed'
} else {
    Say 'not registered'
}

Step 'registry namespace'
if (Test-Path $ProductKey) {
    if (-not $DryRun) { Remove-Item -Path $ProductKey -Recurse -Force }
    Say $ProductKey
} else {
    Say 'nothing written'
}

Step 'files'
if (Test-Path $Prefix) {
    if (-not $DryRun) { Remove-Item -Path $Prefix -Recurse -Force }
    Say $Prefix
} else {
    Say 'nothing there'
}

$settings = Join-Path $env:APPDATA 'Mori Lens'
Step 'settings'
if (Test-Path $settings) {
    if ($Purge) {
        if (-not $DryRun) { Remove-Item -Path $settings -Recurse -Force }
        Say "deleted $settings"
    } else {
        Say "kept $settings"
        Say 'the rules and the language you wrote. -Purge deletes them.'
    }
} else {
    Say 'none written'
}

Write-Host "`nremoved.`n"
