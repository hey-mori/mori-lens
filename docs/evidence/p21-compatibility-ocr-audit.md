# P21: Compatibility + OCR Fallback Audit

**Date**: 2026-09-25  
**Status**: AUDITED, GAPS CLASSIFIED

## Summary

OCR fallback is **safe by design** (manual-only, never automatic fall-through). Compatibility model UI and per-app OCR suggestions are **pre-release hardening candidates**, not release blockers.

## Current State

✅ **Manual-only by design** (`apps/lens/src/manual_ocr.rs`):
- No automatic fall-through from failed hover
- Manual OCR triggered by key press
- Card hidden before capture (prevents self-capture)

✅ **Per-app OCR settings** exist with proper precedence:
```
CLI flag > per-app setting > global setting
```

✅ **Deny policy cannot be weakened** (password managers always denied)

## Gap Classification

| Gap | Classification | Rationale |
|-----|----------------|-----------|
| Compatibility model UI | PRE-RELEASE HARDENING | Nice-to-have, not blocker |
| Per-app OCR suggestion UX | PRE-RELEASE HARDENING | Improves UX, not blocker |
| Automatic OCR capture audit | POST-RELEASE | Moot (no automatic OCR by design) |
| Windows OCR runtime validation | HUMAN GATE (#140) | Requires human operator |

## Recommendation

**Ship v0.1.0 with current OCR implementation.** Design is safe and functional.

## Related

- ADR-0002: Accessibility-first acquisition
- `apps/lens/src/manual_ocr.rs`: Manual OCR implementation
- Issue #140: First-run experience verification
