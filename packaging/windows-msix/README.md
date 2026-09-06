# Windows MSIX Packaging

This directory contains the MSIX packaging infrastructure for the
Microsoft Store distribution channel ([ADR-0017](../../docs/decisions/ADR-0017-signed-binary-distribution.md)).

## Status

- **MSIX packaging structure**: Implemented (this directory)
- **Store submission**: BLOCKED — requires Store developer account
- **Code signing**: Not needed for Store path (Store signs on behalf of publisher)
- **Direct download signing**: BLOCKED — requires OV code-signing certificate

## Files

| File | Purpose |
|---|---|
| `AppxManifest.xml.in` | Package manifest template with placeholder values |
| `Assets/` | Store asset directory (placeholder rules below) |

## Building

```powershell
# On Windows with Windows SDK installed:
.\scripts\package-msix.ps1

# This produces:
#   target\msix\MoriLens_0.1.0.0_x64.msix
```

## Verification

```powershell
.\scripts\verify-msix.ps1 target\msix\MoriLens_0.1.0.0_x64.msix
```

## Store Submission Checklist

Before submitting to the Microsoft Store:

1. Replace all `PLACEHOLDER_*` values in `AppxManifest.xml.in`
2. Provide real Store assets (see Asset Requirements below)
3. Register a Store developer account
4. Run `scripts\package-msix.ps1` to produce the `.msix`
5. Upload to Store via Partner Center

## Asset Requirements

The Store requires specific icon sizes. Place these in `Assets/`:

| File | Size | Required |
|---|---|---|
| `StoreLogo.png` | 50x50 | Yes |
| `Square44x44Logo.png` | 44x44 | Yes |
| `Square150x150Logo.png` | 150x150 | Yes |
| `Wide310x150Logo.png` | 310x150 | Yes |
| `Square310x310Logo.png` | 310x310 | Optional |
| `SplashScreen.png` | 620x300 | Optional |

Currently these are placeholder-sized. Real design assets are an
ASSET BLOCKER tracked in the handoff.

## Publisher Identity

The `Publisher` field in the manifest must match the Store account's
publisher DN exactly. This is determined when the Store developer
account is registered. Until then, `PLACEHOLDER_PUBLISHER_NAME` is used.

## Version Mapping

The MSIX version (`0.1.0.0`) must match the product version in:
- `Cargo.toml` (`version = "0.1.0"`)
- `apps/lens/tauri.conf.json` (`"version": "0.1.0"`)

The build script reads the version from `Cargo.toml` automatically.
