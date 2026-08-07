<#
.SYNOPSIS
    Install Mori Lens for the current user.

.DESCRIPTION
    Copies lens.exe somewhere stable, puts it in the Start Menu, and registers
    it so it appears in Settings > Apps and can be removed from there.

    Per-user throughout. No elevation, no service, no scheduled task, nothing
    written outside this user's own profile and HKCU. That is not modesty about
    what an installer may do: Lens reads the screen of whoever is signed in, so
    an installation shared between two people on one machine would be one
    person's reading tool running in another person's session.

    What this does NOT do, deliberately:

      - It does not start Lens with Windows. That is a tick in the tray menu,
        off unless somebody turns it on, and an installer that switched it on
        would be deciding for them.
      - It does not install Ollama or pull a model. Lens does not bundle a
        model and never will (ADR-0007). It says what to pull, on the card and
        in the tray tooltip, when there is nothing to translate with.
      - It does not touch the firewall. Lens listens on nothing.

.PARAMETER Prefix
    Where to put the binary. Defaults to %LOCALAPPDATA%\Programs\Mori Lens.

.PARAMETER NoShortcut
    Skip the Start Menu entry.

.PARAMETER NoRegister
    Skip the Add/Remove Programs entry.

.PARAMETER DryRun
    Say what would happen and change nothing.

.EXAMPLE
    .\install.ps1

.EXAMPLE
    .\install.ps1 -Prefix D:\Tools\MoriLens
#>
[CmdletBinding()]
param(
    [string] $Prefix = (Join-Path $env:LOCALAPPDATA 'Programs\Mori Lens'),
    [switch] $NoShortcut,
    [switch] $NoRegister,
    [switch] $DryRun
)

$ErrorActionPreference = 'Stop'

$AppName = 'Mori Lens'
# The key name, not the display name. Stable across versions, because an
# uninstall entry keyed on a version leaves one row per version installed.
$RegistryKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\MoriLens'

function Say($message) { Write-Host "  $message" }
function Step($message) { Write-Host "`n· $message" }

$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$source = Join-Path $here 'lens.exe'
if (-not (Test-Path $source)) {
    # The script travels in the zip beside the binary. Run from anywhere else
    # and there is nothing to install, which is worth saying plainly rather
    # than failing on a copy three lines down.
    Write-Host "`nlens.exe is not next to this script."
    Write-Host "Run install.ps1 from the folder it was unpacked into.`n"
    exit 1
}

# A running Lens holds its own image open, so the copy would fail partway and
# leave a half-installed folder. Asked first, and named, because "access is
# denied" on a file the user owns is the least useful message Windows has.
$running = Get-Process -Name 'lens' -ErrorAction SilentlyContinue
if ($running) {
    Write-Host "`nMori Lens is running. Quit it from the tray icon and run this again.`n"
    exit 1
}

# `--version` prints `Mori Lens 0.1.0 · build <sha>`, which is a sentence and
# not a version. Settings > Apps puts DisplayVersion in a narrow column, and
# the whole line there reads as a product whose version number is its name.
#
# The build sha is kept, as semver build metadata, because it is the only thing
# that says which of two 0.1.0 binaries this is and the whole reason `build.rs`
# stamps it.
$banner = (& $source --version 2>$null | Select-Object -First 1)
$version = '0.0.0'
if ($banner -match '(\d+\.\d+\.\d+)') {
    $version = $Matches[1]
    if ($banner -match 'build\s+(\S+)') { $version = $version + '+' + $Matches[1] }
}

Write-Host "`n$AppName installer"
Say "version   $version"
Say "prefix    $Prefix"
Say "scope     this user only, no elevation"
if ($DryRun) { Say 'DRY RUN, nothing will be changed' }

$target = Join-Path $Prefix 'lens.exe'

Step "copying to $Prefix"
if (-not $DryRun) {
    if (-not (Test-Path $Prefix)) {
        New-Item -ItemType Directory -Force -Path $Prefix | Out-Null
    }
    Copy-Item -Path $source -Destination $target -Force
    foreach ($extra in @('READ-ME-FIRST.txt', 'SHA256SUMS.txt', 'uninstall.ps1')) {
        $from = Join-Path $here $extra
        if (Test-Path $from) { Copy-Item -Path $from -Destination $Prefix -Force }
    }
}
Say $target

if (-not $NoShortcut) {
    Step 'Start Menu'
    $menu = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs'
    $link = Join-Path $menu "$AppName.lnk"
    if (-not $DryRun) {
        $shell = New-Object -ComObject WScript.Shell
        $shortcut = $shell.CreateShortcut($link)
        $shortcut.TargetPath = $target
        $shortcut.WorkingDirectory = $Prefix
        $shortcut.Description = 'Rest the pointer on a sentence to see a translation'
        $shortcut.Save()
    }
    Say $link
}

if (-not $NoRegister) {
    Step 'Settings > Apps'
    if (-not $DryRun) {
        New-Item -Path $RegistryKey -Force | Out-Null
        $uninstaller = Join-Path $Prefix 'uninstall.ps1'
        # Quoted, because the default prefix has a space in it and an
        # unquoted path there is two arguments.
        $command = "powershell.exe -NoProfile -ExecutionPolicy Bypass -File `"$uninstaller`""
        Set-ItemProperty -Path $RegistryKey -Name 'DisplayName' -Value $AppName
        Set-ItemProperty -Path $RegistryKey -Name 'DisplayVersion' -Value "$version"
        Set-ItemProperty -Path $RegistryKey -Name 'Publisher' -Value 'swarea'
        Set-ItemProperty -Path $RegistryKey -Name 'InstallLocation' -Value $Prefix
        Set-ItemProperty -Path $RegistryKey -Name 'UninstallString' -Value $command
        Set-ItemProperty -Path $RegistryKey -Name 'QuietUninstallString' -Value $command
        Set-ItemProperty -Path $RegistryKey -Name 'NoModify' -Value 1 -Type DWord
        Set-ItemProperty -Path $RegistryKey -Name 'NoRepair' -Value 1 -Type DWord
        Set-ItemProperty -Path $RegistryKey -Name 'EstimatedSize' `
            -Value ([int]((Get-Item $target).Length / 1KB)) -Type DWord
    }
    Say $RegistryKey
}

Write-Host "`ninstalled. Start it from the Start Menu, or:"
Write-Host "  `"$target`""
Write-Host "`nIt opens no window. Look for the icon in the notification area."
Write-Host "Hold Alt and rest the pointer on a sentence.`n"
Write-Host "Windows may warn on first run: this binary is not signed, and that"
Write-Host "warning is accurate. SHA256SUMS.txt is what you have instead.`n"
