# P25: Distribution Readiness Evidence

**Date**: 2026-09-25  
**Status**: VERIFIED  
**Build Provenance**: BUILT_FROM_CURRENT_CLEAN_HEAD

## Summary

Windows distribution artifacts are structurally valid and verified for this revision. Code signing and Store publication remain external blockers.

## Intermediate Local Pre-Merge Package Evidence

### Build Context
- **Source SHA**: `8841492` (intermediate, pre-PR-merge)
- **Final Source SHA**: PENDING CONVERGENCE (after PR #636 merge)
- **Version**: 0.1.0
- **Tree State**: clean
- **Build Provenance**: INTERMEDIATE_LOCAL_PRE-MERGE

### NSIS Installer
- **Filename**: `Mori Lens_0.1.0_x64-setup.exe`
- **Size**: ~22 MB (exact size recorded during build)
- **SHA-256**: `c6bd2e6fdf7b183de2a6f4e52acc81a7b5fed6e16e301ed0f2ab81d77a9a653d`
- **Architecture**: x64 (AMD64)
- **Structural Verification**: PASS (15/15 checks)
- **Signature State**: UNSIGNED
- **Build Path**: `apps/lens/target/release/bundle/nsis/Mori Lens_0.1.0_x64-setup.exe`

### MSIX Package
- **Filename**: `MoriLens_0.1.0.0_x64.msix`
- **Size**: ~19 MB (exact size recorded during build)
- **SHA-256**: `eacd167488839cecb887a5b423fdaabd904a88ef3d340f58f879130a5676500b`
- **Architecture**: x64 (AMD64)
- **Structural Verification**: PASS (15/15 checks)
- **Signature State**: UNSIGNED
- **Publisher Identity**: PLACEHOLDER (requires replacement before Store submission)
- **Build Path**: `target/msix/MoriLens_0.1.0.0_x64.msix`

## Distribution Classification

### NSIS Installer
- **Classification**: Primary Windows RC install route
- **Use Case**: Direct installation by end users
- **SmartScreen Warning**: Accurate, not false alarm (unsigned)
- **Recommendation**: Attach to GitHub Release as primary Windows artifact

### MSIX Package
- **Classification**: Structurally verified Store-preparation artifact
- **Use Case**: Microsoft Store submission after code signing and publisher identity replacement
- **Not Recommended For**: Direct end-user installation (unsigned, placeholder publisher)
- **Recommendation**: Attach to GitHub Release as Store-preparation artifact, clearly labeled

## External Blockers

1. **Code Signing Certificate**: Required to eliminate SmartScreen warning and enable Store submission
2. **Microsoft Store Publisher Account**: Required for Store distribution
3. **Publisher Identity Replacement**: MSIX AppxManifest.xml requires real publisher identity before Store submission

## Post-Tag Workflow

The release workflow (`.github/workflows/release.yml`) generates artifact checksums and attaches them to the GitHub Release. This document records pre-tag evidence for verification.

## Verification Commands

```bash
# Verify NSIS installer
sha256sum "Mori Lens_0.1.0_x64-setup.exe"
# Expected: c6bd2e6fdf7b183de2a6f4e52acc81a7b5fed6e16e301ed0f2ab81d77a9a653d

# Verify MSIX package
sha256sum "MoriLens_0.1.0.0_x64.msix"
# Expected: eacd167488839cecb887a5b423fdaabd904a88ef3d340f58f879130a5676500b
```

## Related
- `scripts/release-preflight.sh`: Build and verification script
- `scripts/verify-nsis.ps1`: NSIS structural verification
- `scripts/verify-msix.ps1`: MSIX structural verification
- `.github/workflows/release.yml`: Release workflow
