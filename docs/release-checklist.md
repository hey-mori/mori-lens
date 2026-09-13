# Release checklist

Mori Lens releases are versioned `vX.Y.Z` and tagged on `main`. The
release workflow (`.github/workflows/release.yml`) builds all platform
artifacts when a tag is pushed: the Windows NSIS installer, the Windows
MSIX package (for Microsoft Store submission), and the macOS `.app`
bundle. This document covers what the workflow does not: signing,
publishing, and the decisions that go into a release.

## Before tagging

1. Run `scripts/check.sh --full` locally. This runs the full product-quality gate:
   workspace fmt/clippy/test, product binary fmt/clippy/check/test, clean native
   rebuild of lens-platform-macos, release build of hover-probe, privacy static
   gates, documentation integrity, command wiring integrity, and version consistency.
2. Ensure the tree is clean (`git status --short` shows nothing).
3. Run `scripts/release-preflight.sh` on the supported release platform (Windows
   for NSIS/MSIX, macOS for .app bundle). This builds the actual release artifacts
   and verifies their structural validity. The preflight script reports two provenance
   dimensions:

   **BUILD_PROVENANCE** (how the artifact was built):
   - `BUILT_FROM_CURRENT_CLEAN_HEAD` - default mode + clean tree
   - `DIRTY_WORKTREE` - `--allow-dirty` + dirty tree
   - `VERIFY_ONLY` - `--verify-only` mode

   **EMBEDDED_BUILD_STAMP** (what the embedded SHA says):
   - `VERIFIED` - embedded SHA matches current HEAD
   - `UNPROVEN` - no embedded SHA or cannot extract
   - `MISMATCH` - embedded SHA doesn't match

   The preflight script supports:
   - Default mode: builds artifacts and verifies them
   - `--verify-only`: verifies existing artifacts without rebuilding
   - `--source-only`: verifies source-only gates (no artifact build)
   - `--allow-dirty`: allows dirty tree but marks BUILD_PROVENANCE as DIRTY_WORKTREE
4. Verify the version in three places matches:
   - `Cargo.toml` (workspace version)
   - `apps/lens/Cargo.toml`
   - `apps/lens/tauri.conf.json`
5. Update `docs/compatibility/windows-app-matrix.md` if any application
   support changed since the last release.
6. Read `docs/mori-lens-project-goal.md` and confirm the release covers
   what the current milestone promises.

**Important**: `STRUCTURALLY READY` does not mean production ready. The preflight
verifies structural validity of the packages. A release is not ready for distribution
until:
- Packages are code-signed (external blocker)
- Human install smoke test passes on a clean machine
- macOS runtime verification completes (currently PENDING)

## Tagging

```
git tag -a v0.1.0 -m "v0.1.0"
git push origin v0.1.0
```

The tag triggers the release workflow. The workflow:
- Verifies version consistency across all three files.
- Runs `scripts/check.sh --full`.
- Builds the NSIS installer with `cargo tauri build --bundles nsis`.
- Verifies the NSIS installer with `scripts/verify-nsis.ps1`. The verifier checks:
  installer exists and is non-empty, PE header (MZ magic), ProductName and
  FileDescription in version info, ProductVersion and FileVersion match Cargo.toml,
  bootstrap architecture is x86 OR x64 (NSIS commonly uses x86 bootstrap for compatibility),
  payload lens.exe exists and is x64 / AMD64, filename contains version. The verifier
  classifies the Authenticode signature as SIGNED VALID, UNSIGNED, INVALID, or UNKNOWN.
  Regression tests live in `scripts/test_verify_nsis.ps1`.
- Builds the MSIX package with `scripts/package-msix.ps1` and verifies
  it with `scripts/verify-msix.ps1`. The verifier checks: structural
  validity (valid zip, manifest, lens.exe, assets), manifest version
  matches Cargo.toml in 4-part form, SHA-256 matches the `.sha256`
  sidecar, lens.exe PE machine type is AMD64 (0x8664), and classifies
  the Authenticode signature as SIGNED VALID, UNSIGNED, INVALID, or
  UNKNOWN. Publisher identity and PublisherDisplayName are checked for
  placeholder values that must be replaced before Store submission.
  Regression tests live in `scripts/test_verify_msix.ps1`.
- Builds the macOS `.app` bundle with `cargo tauri build --bundles app`.
- Generates `SHA256SUMS.txt` for each artifact.
- Creates a draft GitHub Release with all artifacts and hash manifests
  attached. The release body marks the artifacts as unsigned.

The release is created as a draft. Open it, read the auto-generated
notes, and publish when satisfied.

## Code signing

The installer is not signed. Signing requires a code signing
certificate, which is a purchase and a decision the owner makes. Until
then, Windows SmartScreen warns on first run, and that warning is
accurate rather than a false alarm.

### When a certificate is obtained

1. Export the certificate as a `.pfx` file.
2. Add two GitHub secrets:
   - `WINDOWS_CERTIFICATE`: the base64-encoded `.pfx` file
   - `WINDOWS_CERTIFICATE_PASSWORD`: the password for the `.pfx`
3. Update `.github/workflows/release.yml` to decode the certificate
   before `cargo tauri build` and pass signing options to Tauri.

The Tauri v2 NSIS bundler accepts `signCommand` in the NSIS
configuration. The command signs both the installer and the embedded
binary.

## Auto-update

Tauri v2 has a built-in updater that checks a publishing endpoint for
new versions. The updater needs:

1. A publishing endpoint that serves a JSON manifest.
2. An Ed25519 keypair for signing releases.
3. The public key baked into the application at build time.

### Setting up the updater

1. Generate a keypair:
   ```
   cargo install tauri-cli
   cargo tauri signer generate -w ~/.tauri/mori-lens.key
   ```
2. Add the private key as a GitHub secret: `TAURI_SIGNING_PRIVATE_KEY`.
3. Add the passphrase as a GitHub secret:
   `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.
4. Add the public key to `apps/lens/tauri.conf.json` under
   `bundle.updater.pubkey`.
5. Configure the updater endpoint in the application code.

The publishing endpoint is infrastructure the owner provides. Until it
exists, there is no auto-update, and users download new versions from
the GitHub Releases page.

## What a release contains

A Mori Lens release contains three artifacts:

- **Windows NSIS installer** (`.exe`): installs per-user with no admin
  rights needed. Places the binary in
  `%LOCALAPPDATA%\Programs\Mori Lens`, adds a Start Menu shortcut, and
  registers in Settings > Apps for uninstall.
- **Windows MSIX package** (`.msix`): structurally valid for Microsoft
  Store submission. Publisher identity uses placeholder values that
  must be replaced before Store submission.
- **macOS `.app` bundle** (zipped): a signed-ready application bundle.
  Gatekeeper blocks the unsigned bundle until the user right-clicks
  and opens it, or verifies its SHA-256.

None of the artifacts:
- Start with Windows by default (a tray menu option).
- Bundle a model (Ollama is a separate install).
- Send telemetry or make network requests on their own.

## After publishing

1. Verify the installer downloads and installs on a clean machine.
2. Verify the SHA-256 checksum matches the `SHA256SUMS.txt` attached to
   the release.
3. Update `docs/compatibility/windows-app-matrix.md` with the build
   date and version if any probe runs were done.
