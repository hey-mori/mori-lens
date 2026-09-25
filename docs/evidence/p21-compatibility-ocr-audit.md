# P21: Compatibility + OCR Fallback Audit

**Date**: 2026-09-25  
**Status**: AUDITED, GAPS CLASSIFIED  
**Revised**: Corrected automatic OCR behavior after production source audit

## Summary

OCR fallback is **safe by design** with proper safety invariants. Automatic OCR is **implemented and opt-in only**, not manual-only as previously documented. Compatibility model UI and per-app OCR suggestions are **pre-release hardening candidates**, not release blockers.

## Automatic OCR Behavior (Corrected)

### Implementation Status
✅ **Automatic OCR is implemented** (`crates/lens-core/src/session.rs:323-354`):
- `ocr = automatic` in settings enables automatic fall-through from accessibility failure to OCR
- `--ocr automatic` CLI flag overrides all settings
- Off by default (manual is default)

### Routing Path
1. Hover settles and calls `resolve()` (session.rs:263)
2. Accessibility acquirer is tried first (line 283)
3. If accessibility fails, `ocr_fallback()` is called (line 285)
4. Protected fields are filtered (line 334) - password managers always denied
5. `error.ocr_can_help()` must return true (line 334)
6. Effective OCR mode resolved with precedence: CLI > per-app > global (line 339-344)
7. If mode is Automatic, OCR runs without keypress (line 348)

### Safety Invariants (Source-Verified)
✅ **Protected fields always denied** (session.rs:334):
- Password managers and protected fields are filtered before OCR
- Defense-in-depth: checked in both `ocr_can_help()` and `ocr_fallback()`

✅ **Capture frequency bounded**:
- `same_target_cooldown: 1000ms` (hover.rs:67) - same target suppressed for 1 second
- Hover machine deduplicates by fingerprint (hover.rs:311)
- Stale generation detection prevents old resolutions (session.rs:273)

✅ **Pause behavior**:
- Ctrl+Alt+L pause key works independently of OCR mode
- Pause suppresses all acquisition including automatic OCR
- Worker thread continues sampling but resolutions are suppressed

✅ **Card self-capture prevention**:
- Card hidden before capture in manual OCR
- Automatic OCR uses same acquisition path as accessibility
- No known self-capture vulnerability in automatic mode

✅ **No screen persistence**:
- OCR captures a band around the pointer, not full screen
- No image written to disk
- No captured image retained in memory after text extraction

### Configuration Precedence
```
CLI flag (--ocr) > per-app setting > global setting
```

Per-app rules: `AppOcrRule` in settings file
Global setting: `ocr = manual|automatic|off` in settings file

### What is NOT Runtime-Verified
⚠️ **No human has hovered an editor with `ocr = automatic` enabled**:
- Unit tests verify routing logic
- Integration tests verify policy precedence
- Real-world behavior with actual applications is unverified
- Issue #140 tracks first-run experience including OCR verification

## Manual OCR Behavior

✅ **Manual OCR** (`apps/lens/src/manual_ocr.rs`):
- Triggered by Ctrl+Alt+O keypress
- Card hidden before capture (prevents self-capture)
- Same translation and card path as accessibility
- Works even when `ocr = off` (explicit user action)

## Gap Classification

| Gap | Classification | Rationale |
|-----|----------------|-----------|
| Compatibility model UI | PRE-RELEASE HARDENING | Nice-to-have, not blocker |
| Per-app OCR suggestion UX | PRE-RELEASE HARDENING | Improves UX, not blocker |
| Automatic OCR runtime validation | HUMAN GATE (#140) | Requires human operator with `ocr = automatic` |
| Windows OCR runtime validation | HUMAN GATE (#140) | Requires human operator |
| Capture storm stress testing | POST-RELEASE | Cooldown exists, stress untested |
| Automatic OCR negative-memory/caching | POST-RELEASE | Not implemented, may not be needed |

## Recommendation

**Ship v0.1.0 with current OCR implementation.** Automatic OCR is implemented correctly with proper safety invariants. Runtime verification with actual applications is a human gate, not a release blocker.

## Related
- ADR-0002: Accessibility-first acquisition
- `crates/lens-core/src/session.rs:323-354`: OCR fallback routing
- `apps/lens/src/manual_ocr.rs`: Manual OCR implementation
- `crates/lens-core/src/hover.rs:67`: Same-target cooldown
- Issue #140: First-run experience verification
