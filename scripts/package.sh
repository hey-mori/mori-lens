#!/usr/bin/env bash
#
# Build a release someone else can run.
#
#   scripts/package.sh          package the current commit
#   scripts/package.sh --dirty  package a working tree with changes in it
#
# What this is for, and what it is not. `docs/mori-lens-project-goal.md` named
# two missing pieces before Lens is something a person installs: a signed
# binary and an installer. The installer now travels in the zip, as
# `install.ps1`, so one of the two is done.
#
# Signing is the other and it stays missing. It needs a certificate, which is a
# purchase and a decision, and until there is one Windows will warn on first
# run. That warning is honest and this script does not try to talk anybody out
# of it: the README says so, and so do the installer and the file this writes
# beside the binary.
#
# # Why it refuses a dirty tree by default
#
# A release names a commit. A release built from a working tree with edits in
# it names a commit it is not, and the sha in `--version` becomes a lie that
# looks exactly like the truth. That is the failure this repository spent a
# session on when the product had no build stamp at all. `--dirty` is there
# because packaging a work in progress to hand to somebody is a real thing to
# want, and it marks the artifact rather than pretending.
set -euo pipefail

cd "$(dirname "$0")/.."

ALLOW_DIRTY=0
[[ "${1:-}" == "--dirty" ]] && ALLOW_DIRTY=1

step() { printf '\n· %s\n' "$1"; }

SHA="$(git rev-parse --short HEAD)"
VERSION="$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)"
SUFFIX=""

if [[ -n "$(git status --porcelain)" ]]; then
  if [[ "$ALLOW_DIRTY" == "0" ]]; then
    printf '\nthe working tree has changes, so a package would name a commit it is not.\n'
    printf 'commit them, or run with --dirty to mark the artifact instead.\n\n'
    git status --short
    exit 1
  fi
  SUFFIX="-dirty"
fi

NAME="mori-lens-${VERSION}-${SHA}${SUFFIX}-windows-x64"
OUT="target/package/${NAME}"

# The gate first. A package that has not been verified is a package somebody
# else finds the defect in.
step "scripts/check.sh"
scripts/check.sh

step "cargo build --release -p lens"
cargo build --release -p lens

step "assembling ${NAME}"
rm -rf "$OUT"
mkdir -p "$OUT"
cp target/release/lens.exe "$OUT/"
# The installer travels with the binary rather than being fetched. A person who
# has a zip and no network still has both halves, and an installer downloaded
# separately from the thing it installs is a supply chain with two ends.
cp scripts/install.ps1 scripts/uninstall.ps1 "$OUT/"

# What a person needs beside the binary, and nothing else. Not the README:
# most of it is about working on Lens rather than running it, and a file that
# is mostly irrelevant is a file nobody reads to the part that matters.
cat > "$OUT/READ-ME-FIRST.txt" <<TXT
Mori Lens ${VERSION}
build ${SHA}${SUFFIX}

Rest the pointer on a sentence and a card appears beside it with a
translation. No selecting, no copying, no hotkey.

BEFORE IT CAN TRANSLATE ANYTHING

  Lens does not bundle a model and never will. Install Ollama from
  ollama.com, then, in a terminal:

      ollama pull qwen2.5:7b

  Without this Lens still starts and tells you exactly this, on the card
  and in the tray tooltip. It does not fail silently.

INSTALLING IT, OR NOT

  Either works. lens.exe is the whole product: one file, no service, no
  elevation. Put it wherever you keep such things and run it.

  install.ps1 is there if you would rather it behaved like an installed
  program. Right-click it and choose Run with PowerShell, or:

      powershell -ExecutionPolicy Bypass -File install.ps1

  It copies the binary to %LOCALAPPDATA%\Programs\Mori Lens, adds a Start
  Menu entry, and registers it so it appears in Settings > Apps and can be
  removed from there. This user only. It does not ask for administrator, it
  does not install a service, and it does not switch on start-with-Windows:
  that is a tick in the tray menu and it is yours to make.

  uninstall.ps1 reverses all of it and keeps your settings file. Add -Purge
  to delete that too.

RUNNING IT

  It opens no window and appears in the notification area.

  Hold Alt and rest the pointer on a sentence.
  Ctrl+Alt+L pauses. Paused means no accessibility call is made at all.
  Ctrl+Alt+O reads the pixels around the pointer, where an application
    exposes no text. Never automatic, and no image is ever written anywhere.
  Ctrl+Alt+D explains the sentence on the card: the dictionary form of the
    word you were on, its part of speech, what it means here, the structure.
  Ctrl+Alt+C copies the card. Lens never reads your clipboard, only writes
    to it, and only when you press this.
  Right-click the tray icon for the settings file, start-with-Windows, quit.

  lens.exe --version   says which build this is
  lens.exe --help      says what else it takes

WINDOWS WILL WARN ON FIRST RUN

  This binary is not signed. Signing needs a certificate that has not been
  bought, so SmartScreen will say the publisher is unknown, and that is
  accurate rather than a false alarm. Check the SHA-256 below against the
  one published with this release before you decide to trust it.

WHAT IT SENDS ANYWHERE

  Nothing. Translation runs against an Ollama on this machine, at
  127.0.0.1:11434 unless you change it, and the endpoint setting refuses an
  address that is not on this machine. Nothing is written to disk except
  the settings file you can read. Screen captures are never written at all.

  The full account is docs/privacy-model.md in the repository.
TXT

step "checksum"
if command -v sha256sum >/dev/null 2>&1; then
  (cd "$OUT" && sha256sum lens.exe > SHA256SUMS.txt)
else
  # Windows without the GNU tools. PowerShell is always there.
  powershell -NoProfile -Command \
    "(Get-FileHash -Algorithm SHA256 '$OUT/lens.exe').Hash.ToLower() + ' *lens.exe'" \
    > "$OUT/SHA256SUMS.txt"
fi
cat "$OUT/SHA256SUMS.txt"

step "zip"
ZIP="target/package/${NAME}.zip"
rm -f "$ZIP"
if command -v zip >/dev/null 2>&1; then
  (cd target/package && zip -qr "${NAME}.zip" "${NAME}")
else
  powershell -NoProfile -Command \
    "Compress-Archive -Path '$OUT' -DestinationPath '$ZIP' -Force"
fi

printf '\npackaged  %s\n' "$ZIP"
printf 'unsigned  Windows will warn on first run, and it is right to\n'
