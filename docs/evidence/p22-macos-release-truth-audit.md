# P22: macOS Release Support Truth Audit

**Date**: 2026-09-25  
**Phase**: Release Candidate Preparation  
**Status**: CODE COMPLETE, RUNTIME VERIFICATION = PENDING (human/hardware gate)

## Summary

Mori Lens has **complete macOS code** but **no real-world runtime verification**. The code compiles, passes CI, and builds .app bundles, but has never been tested on actual macOS hardware with real applications.

## What Exists

### Code (Complete)
- **`crates/platform-macos/`**: Full Objective-C adapter (32 files)
  - Accessibility acquisition (AX API)
  - Vision framework OCR
  - ScreenCaptureKit integration
  - NSPanel overlay with click-through
  - Hotkey handling
  - Menubar integration
  - Clipboard management
  - Application lifecycle

- **`apps/lens/src/macos.rs`**: Product entry point (~1900 lines)
  - Complete macOS-specific logic
  - Permission request flows
  - Overlay management
  - Engine integration

### Build Configuration (Complete)
- `tauri.conf.json`: macOS bundle config
  - `minimumSystemVersion: 12.3`
  - `.app` bundle target
  - `icon.icns` configured
- GitHub Actions: Builds macOS .app on `macos-latest` runners
- Structural validation: `scripts/verify-app-bundle.sh`, `scripts/validate-macos.sh`

### Documentation (Complete)
- `docs/roadmap.md`: M6/M7 marked "IMPLEMENTATION: complete" with detailed status tables
- `docs/architecture.md`: Fully documents macOS adapter design
- ADRs cover macOS-specific decisions

## What's Verified (Automated)

✅ **Unit tests**: Coordinate conversion, UTF conversion, capture math, permission state  
✅ **CI passes**: `scripts/check.sh` runs on macOS in CI  
✅ **Bundle structure**: Verified by release workflows  
✅ **Code compiles**: Links successfully on macOS  

## What's NOT Verified (Real-World Runtime)

⏳ **Running the product on actual macOS hardware**  
⏳ **Accessibility permission grant/flow**  
⏳ **Screen Recording permission flow**  
⏳ **Real overlay visibility, click-through, focus behavior**  
⏳ **Real Retina coordinate accuracy**  
⏳ **Real Spaces behavior**  
⏳ **Acquisition against real macOS apps** (Safari, TextEdit, VS Code, etc.)  
⏳ **No `docs/compatibility/macos-app-matrix.md`** exists (unlike Windows)  

## "MACOS RUNTIME VERIFIED = NO" Accuracy

**ACCURATE.** Multiple documents confirm this:
- `handoff.md:61`: "Real macOS product build/run verification: PENDING"
- `docs/release-checklist.md:51`: "macOS runtime verification completes (currently PENDING)"
- `docs/post-v1-product-hardening-roadmap.md:289`: "When macOS hardware/runtime available, verify..."
- Roadmap M6/M7 tables: all real-world conditions marked "AWAITING"

## What's Needed for macOS Release Support

### Hardware/Environment
1. macOS hardware (MacBook, iMac, Mac mini, or VM with passthrough)
2. macOS 12.3 or later
3. Accessibility permission grant capability
4. Screen Recording permission grant capability

### Verification Steps
1. Build .app bundle locally: `cargo tauri build --bundles app`
2. Run product on macOS hardware
3. Grant Accessibility permission
4. Grant Screen Recording permission
5. Test against real apps: Safari, TextEdit, VS Code, Terminal, Notes
6. Record results in `docs/compatibility/macos-app-matrix.md`
7. Verify overlay behavior (visibility, click-through, focus)
8. Verify Retina coordinate accuracy
9. Verify Spaces behavior
10. Complete the "real-world measurement" column in roadmap M6/M7 tables

### Distribution (External Blocker)
1. Code signing certificate for macOS (Apple Developer Program)
2. Notarization setup
3. Gatekeeper bypass instructions for unsigned builds

## Classification

**RELEASE BLOCKER**: NO (macOS is not required for initial release)  
**PRE-RELEASE HARDENING**: YES (needed for macOS support claim)  
**HUMAN GATE**: YES (requires physical macOS hardware)  

## Recommendation

Ship Windows release first (P25/P26/P27/P28 can complete without macOS). macOS release support is a post-v1.0 milestone unless macOS hardware becomes available before Windows release is finalized.

## Related

- `docs/roadmap.md`: M6/M7 status tables
- `docs/release-checklist.md`: macOS runtime verification requirement
- `docs/compatibility/windows-app-matrix.md`: Windows equivalent (exists)
- Issue #622: Release candidate preparation tracking
