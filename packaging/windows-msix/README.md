# Windows MSIX Packaging

This directory contains the MSIX packaging infrastructure for the
Microsoft Store distribution channel ([ADR-0017](../../docs/decisions/ADR-0017-signed-binary-distribution.md)).

## Status

- **MSIX packaging structure**: Implemented and verified locally
- **CI integration**: Release workflow builds and verifies MSIX on tag push
- **Store submission**: BLOCKED on Store developer account registration
- **Code signing**: Not needed for Store path (Store signs on behalf of publisher)
- **Direct download signing**: BLOCKED on OV code-signing certificate

## Files

| File | Purpose |
|---|---|
| `AppxManifest.xml.in` | Package manifest template with placeholder values |
| `Assets/` | Store icon assets, resized from `apps/lens/icons/icon.png` |

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

A structurally valid package reports 11 passed, 0 failed, 2 warnings
(the warnings are the placeholder publisher values, which are replaced
before Store submission).

## Store Submission Checklist

Before submitting to the Microsoft Store:

1. Replace all `PLACEHOLDER*` values in `AppxManifest.xml.in`
2. Register a Store developer account
3. Run `scripts\package-msix.ps1` to produce the `.msix`
4. Upload to Store via Partner Center

## Publisher Identity

The `Publisher` field in the manifest must match the Store account's
publisher DN exactly. This is determined when the Store developer
account is registered. Until then, `PLACEHOLDERPUBLISHERNAME` is used.

## Version Mapping

The MSIX version (`0.1.0.0`) must match the product version in:
- `Cargo.toml` (`version = "0.1.0"`)
- `apps/lens/tauri.conf.json` (`"version": "0.1.0"`)

The build script reads the version from `Cargo.toml` automatically.
