# Repository Gap Analysis — 2026-09-09

**HISTORICAL SNAPSHOT** — Created: 2026-09-09

> This document identifies gaps as of the above date.
> Many gaps have been addressed in P16 (shared orchestration) and subsequent work.
> For current state, see:
> - [Post-v1 product hardening roadmap](post-v1-product-hardening-roadmap.md)
> - [Capability evidence matrix](capability-evidence-matrix.md)
> - Recent commits: `git log --oneline -20`

## Executive Summary

Handoff claims "all repository-local implementation complete except hover_scope on macOS" is **FALSE**.

Actual state: **18 critical gaps** identified across macOS lifecycle, settings architecture, and ADR-0005 compliance.

## Critical Product Bugs (P0 — breaks functionality)

### 1. macOS Modifier Policy Bug
**Location:** `apps/lens/src/macos.rs:894`
**Issue:** `policy.allows(process_name.as_deref(), false)` — modifier_held hardcoded to false
**Impact:** With default `Modifier` tier, NO applications are allowed on macOS
**Fix:** Track Option/Alt key state, pass to policy check
**Test:** Requires macOS hardware

### 2. macOS --settings Parsed But Unused
**Location:** `apps/lens/src/macos.rs:91` (parse), `:160` (use)
**Issue:** `flags.settings` parsed but `data_dir` hardcoded to `~/Library/Application Support/Mori Lens`
**Impact:** Advertised CLI feature doesn't work
**Fix:** Use `flags.settings` to override data_dir or settings path
**Test:** Requires macOS hardware

### 3. macOS --auto Parsed But Unused
**Location:** `apps/lens/src/macos.rs:90`
**Issue:** `flags.auto` parsed but never applied to policy
**Impact:** Advertised CLI feature doesn't work
**Fix:** Apply `--auto` to set default tier to Allow
**Test:** Requires macOS hardware

### 4. Windows Settings Parser Conflict
**Location:** `apps/lens/src/win.rs:351` (legacy), `:1140` (new)
**Issue:** Both `Settings::parse()` and `settings_service::load_or_migrate()` read same file
**Impact:** First-run writes legacy TEMPLATE, restart fails with MissingVersion error
**Fix:** Unify on single canonical parser (lens-settings)
**Test:** Can test on Windows

## Architectural Gaps (P1 — maintenance burden)

### 5. macOS Doesn't Use HoverSession/HoverWorker
**Location:** `apps/lens/src/macos.rs` (entire file)
**Issue:** Reimplements hover logic instead of using shared core contract
**Impact:** Duplicates Windows bug fixes, misses generation tracking, stale result safety
**Fix:** Integrate HoverSession/HoverWorker like Windows does
**Test:** Requires macOS hardware

### 6. hover_worker.rs Is Windows-Only
**Location:** `apps/lens/src/win.rs:34`
**Issue:** Declared as submodule of win.rs, not shared
**Impact:** Cannot reuse on macOS
**Fix:** Move to shared module in main.rs
**Test:** Can verify on Windows

### 7. macOS Acquisition Blocks Pause/Quit
**Location:** `apps/lens/src/macos.rs` (product loop)
**Issue:** No worker thread architecture, acquisition runs in main loop
**Impact:** 500ms AX call blocks pause key for 500ms
**Fix:** Use HoverWorker like Windows
**Test:** Requires macOS hardware

### 8. macOS Parallel Global Slots
**Location:** `apps/lens/src/macos.rs` (multiple variables)
**Issue:** `current_card`, `current_app`, `pending_app`, `current_anchor`, etc. move independently
**Impact:** App/focus/provenance drift bugs (already fixed 3 times)
**Fix:** Bundle into ReadingContext struct
**Test:** Requires macOS hardware

### 9. macOS No Stale Result Safety
**Location:** `apps/lens/src/macos.rs` (result handling)
**Issue:** No generation tracking like HoverSession provides
**Impact:** Old results can reappear after pointer leaves
**Fix:** Use HoverSession generation tracking
**Test:** Requires macOS hardware

## Settings Architecture (P1 — foundational)

### 10. Two Settings Parsers on Windows
**Location:** `crates/lens-core/src/settings.rs` + `crates/lens-settings/src/lib.rs`
**Issue:** Legacy parser (lens-core) and new parser (lens-settings) both read settings.toml
**Impact:** Maintenance burden, potential drift
**Fix:** Retire legacy parser from production use, keep only for migration
**Test:** Can test on Windows

### 11. Legacy First-Run Writes Incompatible Format
**Location:** `apps/lens/src/win.rs:370`
**Issue:** Writes `settings::TEMPLATE` (old flat format) on first run
**Impact:** New parser rejects it (MissingVersion error)
**Fix:** Write new TOML format from first run
**Test:** Can test on Windows

### 12. Settings File Permissions Bypass
**Location:** `apps/lens/src/win.rs:370`
**Issue:** Uses `std::fs::write()` instead of secure atomic writer
**Impact:** 0600 permission guarantee bypassed on first run
**Fix:** Use canonical writer for all settings writes
**Test:** Can test on Unix

## ADR-0005 Compliance (P2 — feature completeness)

### 13. Quick/Deep Not Truly Separate
**Location:** `apps/lens/src/win.rs` (translator creation)
**Issue:** Need to verify if Quick and Deep use independent models/endpoints
**Impact:** ADR-0005 claims "separate models" but may not be implemented
**Fix:** Ensure separate translator instances with separate configs
**Test:** Can test on Windows

### 14. macOS Deep Cache Missing
**Location:** `apps/lens/src/macos.rs`
**Issue:** PR #342 removed unused `_analysis_cache`, no Deep cache exists
**Impact:** ADR-0005 requires separate Deep cache
**Fix:** Implement AnalysisCache for macOS Deep lane
**Test:** Requires macOS hardware

## macOS Parity (P2 — feature gaps)

### 15. Quick Focused-Word Parity Unknown
**Location:** `apps/lens/src/macos.rs` (QuickRequest creation)
**Issue:** Need to verify if macOS QuickRequest carries focused_word
**Impact:** Windows has it (PR #340), macOS may not
**Fix:** Add focused_word to QuickRequest if missing
**Test:** Requires macOS hardware

### 16. hover_scope Not Implemented
**Location:** `apps/lens/src/macos.rs`
**Issue:** Confirmed by handoff — no Ctrl key tracking for scope toggle
**Impact:** Feature gap vs Windows
**Fix:** Implement Ctrl tracking + scope toggle logic
**Test:** Requires macOS hardware

## Validation & Testing (P3 — quality assurance)

### 17. No Adversarial Endpoint Testing
**Location:** `crates/lens-core/src/translate.rs` (engine_for)
**Issue:** Need to test `127.0.0.1.evil`, `127.0.0.999`, etc.
**Impact:** Weak classifier could be bypassed
**Fix:** Add test cases for adversarial endpoints
**Test:** Can test on Windows

### 18. Settings Validation Drift
**Location:** Multiple files
**Issue:** Different validators in read/write/runtime paths
**Impact:** Value accepted by one path rejected by another
**Fix:** Unify validation logic
**Test:** Can test on Windows

## Dependency Graph

```
Phase 10-12 (Settings unification)
  ↓
Phase 13-15 (ADR-0005 implementation)
  ↓
Phase 2-9 (macOS lifecycle fixes)
  ↓
Phase 16-18 (Validation sweeps)
```

## Estimated Effort

- Phase 10-12: 8-12 hours (Windows-testable)
- Phase 13-15: 6-8 hours (Windows-testable)
- Phase 2-9: 16-20 hours (requires macOS testing)
- Phase 16-18: 4-6 hours (Windows-testable)
- **Total: 34-46 hours**

## Recommended PR Order

1. **PR 1:** fix/settings-single-owner (Phase 10-12)
   - Windows-only, fully testable
   - Foundational for other work
   
2. **PR 2:** feat/two-speed-runtime-config (Phase 13-15)
   - Windows-only, fully testable
   - Depends on settings unification
   
3. **PR 3:** fix/macos-session-input-lifecycle (Phase 2-5)
   - Requires macOS testing
   - Most critical macOS bugs
   
4. **PR 4:** fix/macos-reading-context-and-scope (Phase 6-9)
   - Requires macOS testing
   - Depends on PR 3
   
5. **PR 5:** fix/macos-deep-cache (Phase 15)
   - Requires macOS testing
   
6. **PR 6:** docs/current-product-truth (Phase 19)
   - Update handoff, ADR status, etc.

## Current Session Constraints

- **Platform:** Windows only (C:\dev\mori-lens)
- **Can test:** PR 1, PR 2, PR 6 fully; PR 3-5 code only
- **Cannot test:** macOS runtime behavior
- **Time available:** Unknown, but scope is 34-46 hours

## Immediate Action Plan

1. ✅ Phase 1 audit (COMPLETE)
2. Start Phase 10 (settings conflict proof)
3. Implement Phase 10-12 (settings unification)
4. Create PR 1
5. Continue to Phase 13-15
6. Document macOS fixes with test plans
7. Leave macOS runtime validation for when hardware is available
