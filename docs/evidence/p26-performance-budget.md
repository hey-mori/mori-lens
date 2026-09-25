# P26 Performance Budget

**Date**: 2026-09-25
**Status**: DOCUMENTED (subset measured, subset target, subset unknown)
**Scope**: Startup, idle, acquisition, translation, Library queries

## Executive Summary

Performance characteristics are documented with **truthful evidence levels**:
- **MEASURED**: Values from benchmarks (cited with source file, date, platform)
- **TARGET**: Product goals, not yet validated by measurement
- **UNKNOWN**: No measurement exists

No flaky wall-clock CI gates. Deterministic tests for stable properties only (timeouts, pagination, event delegation). Informational measurements for variable properties.

**This document does not claim all values are measured.** Unknown is a valid evidence state.

## Evidence Level Key

- ✅ **MEASURED**: Benchmark exists, cited below
- 🎯 **TARGET**: Product goal, not validated
- ⚠️ **UNKNOWN**: No measurement
- 🔒 **DETERMINISTIC GUARD**: Tested in CI (not wall-clock)

## Startup Performance

### Tray Icon Appearance

**Evidence Level**: ⚠️ UNKNOWN

**Target Budget**: <1000ms to tray icon

**Measurement**: None exists

**Regression Guard**: Informational only (platform variability too high)

### Control Center First Open

**Evidence Level**: ⚠️ UNKNOWN

**Target Budget**: <800ms first open, <200ms subsequent

**Measurement**: None exists

**Regression Guard**: Informational only

## Idle Resource Usage

### CPU (Idle, Pointer Stationary)

**Evidence Level**: ⚠️ UNKNOWN

**Target Budget**: <3% idle, <5% pointer-moving

**Measurement**: None exists (platform variability)

**Deterministic Guard**: ✅ Event delegation test prevents listener accumulation (`apps/lens/web/js/accessibility-production.vitest.test.js`)

### Memory (Idle)

**Evidence Level**: ⚠️ UNKNOWN

**Target Budget**: <200MB for typical session

**Measurement**: None exists (GC/platform variability)

**Regression Guard**: Informational only

## Acquisition Latency

### Settled Pointer → Acquisition Start

**Evidence Level**: 🔒 DETERMINISTIC GUARD

**Mechanism**:
- Dwell time: 300ms (configurable)
- Movement radius: 8px (configurable)
- Hover machine state transitions tested

**Target Budget**: <500ms total (dwell + trigger)

**Deterministic Guard**: ✅ Hover machine tests in `crates/lens-core/src/hover.rs`

### Accessibility Acquisition (UI Automation)

**Evidence Level**: 🔒 DETERMINISTIC GUARD (timeout)

**Mechanism**:
- Per-call timeout: 800ms enforced in production
- Actual latency varies by app (50-500ms observed anecdotally)

**Target Budget**: <800ms (timeout is the guard)

**Deterministic Guard**: ✅ Per-call timeout enforced in `crates/platform-windows/src/uia.rs`

**Note**: No benchmark of actual UIA latency across apps exists for v0.1.0.

### OCR Acquisition (Windows OCR)

**Evidence Level**: 🔒 DETERMINISTIC GUARD (timeout)

**Mechanism**:
- Per-call timeout: 1500ms enforced in production
- Actual latency varies (200-800ms observed anecdotally)

**Target Budget**: <1500ms (timeout is the guard)

**Deterministic Guard**: ✅ Per-call timeout enforced in `crates/platform-windows/src/ocr_acquire.rs`

**Note**: No benchmark of actual OCR latency exists for v0.1.0.

## Translation Performance

### Quick (Warm, Local Provider)

**Evidence Level**: ✅ MEASURED (Ollama only)

**Measurement**:
- Source: `docs/evidence/p20e-live-provider-evidence.md`
- Provider: Ollama qwen2.5:7b
- Platform: Windows 11
- Date: 2026-09-25
- Median: 2-5 seconds (warm)

**Target Budget**: <10 seconds (warm, local provider)

**Note**: LM Studio latency is UNKNOWN for v0.1.0. P20E states "LIVE LM STUDIO VERIFIED = NO".

### Cold Quick (Provider Not Loaded)

**Evidence Level**: ⚠️ UNKNOWN

**Target Budget**: <45 seconds total (cold start + inference)

**Measurement**: None exists (provider variability too high)

**Regression Guard**: Informational only

### Deep (Explanation)

**Evidence Level**: ⚠️ UNKNOWN

**Target Budget**: <60 seconds

**Measurement**: None exists (context length variability)

**Regression Guard**: Informational only

### Manual OCR (User-Triggered)

**Evidence Level**: ⚠️ UNKNOWN

**Target Budget**: <10 seconds (OCR + translation)

**Measurement**: None exists

**Regression Guard**: Informational only

### Automatic OCR (Fallback)

**Evidence Level**: ⚠️ UNKNOWN

**Target Budget**: <10 seconds

**Measurement**: None exists (same code path as manual OCR)

**Regression Guard**: Informational only

## Library Performance

### Query (First Page, No Filters)

**Evidence Level**: ⚠️ UNKNOWN

**Target Budget**: <500ms (even for very large libraries)

**Measurement**: None exists

**Deterministic Guard**: ✅ Pagination correctness tests (11 tests in `crates/lens-storage/src/lib.rs`)

**Note**: Offset pagination is not O(1); performance degrades with offset. Acceptable for v0.1.0 typical sizes.

### Query (With Filters)

**Evidence Level**: ⚠️ UNKNOWN

**Target Budget**: <1000ms (even for complex filters)

**Measurement**: None exists

**Deterministic Guard**: ✅ Filter + pagination interaction tests

### Search (FTS or LIKE)

**Evidence Level**: ⚠️ UNKNOWN

**Target Budget**: <500ms

**Measurement**: None exists

**Regression Guard**: Informational only

## Control Center Performance

### Idle (Open, No Interaction)

**Evidence Level**: ⚠️ UNKNOWN

**Target Budget**: <2% CPU, stable memory

**Measurement**: None exists

**Deterministic Guard**: ✅ Event delegation test prevents listener accumulation

### Settings Change → Reconfigure

**Evidence Level**: ⚠️ UNKNOWN

**Target Budget**: <200ms

**Measurement**: None exists

**Regression Guard**: Informational only

## Deterministic Regression Guards

### Implemented Tests (Run in CI)

1. **Event delegation** (no listener accumulation)
   - Location: `apps/lens/web/js/accessibility-production.vitest.test.js`
   - Property: single listener pattern prevents accumulation

2. **Stable DOM identity** (no unnecessary re-renders)
   - Location: `apps/lens/web/js/accessibility-production.vitest.test.js`
   - Property: DOM nodes retain identity across updates

3. **Hover machine state transitions**
   - Location: `crates/lens-core/src/hover.rs` (tests)
   - Property: dwell/movement radius enforced

4. **Pagination correctness**
   - Location: `crates/lens-storage/src/lib.rs` (11 tests)
   - Property: has_more, total, offset all correct

5. **Per-call timeouts**
   - Location: `crates/platform-windows/src/uia.rs`, `crates/platform-windows/src/ocr_acquire.rs`
   - Property: acquisition bounded by timeout

### Not Implemented (By Design)

- Wall-clock CI gates (too flaky across platforms)
- Memory growth tests (GC variability)
- Network latency tests (provider variability)
- Startup time tests (platform variability)

## Performance Summary by Evidence Level

### Measured (Benchmark Exists)

- Quick translation (Ollama warm): 2-5s median (P20E)

### Deterministic Guards (Tested in CI)

- Hover machine state transitions
- Pagination correctness (has_more, total, offset)
- Event delegation (no listener accumulation)
- Per-call timeouts (UIA 800ms, OCR 1500ms)

### Target Only (No Measurement)

- Startup time (<1000ms tray, <800ms Control Center)
- Idle CPU (<3%)
- Idle memory (<200MB)
- Translation cold start (<45s)
- Deep explanation (<60s)
- Library queries (<500-1000ms)

### Unknown (No Benchmark, No Target)

- LM Studio latency
- Very large library (>10k items) performance
- Review session with >1000 items
- Concurrent provider downloads

## Conclusion

Performance budget is **documented with truthful evidence levels**:

- **1 measured value**: Ollama Quick translation (P20E)
- **5 deterministic guards**: Hover, pagination, event delegation, timeouts
- **10+ targets**: Startup, idle, translation, library queries
- **4+ unknowns**: LM Studio, large-scale performance

**No release-blocking defects found.** The document does not claim more than it can prove.

**Recommendation**: Ship v0.1.0 with current evidence. Monitor performance in production. Add benchmarks for unknown values in future releases.

## Related

- `docs/evidence/p20e-live-provider-evidence.md` (Ollama measurement)
- `docs/evidence/p27-library-boundedness-audit.md` (pagination details)
- `crates/lens-core/src/hover.rs` (hover machine tests)
- `crates/lens-storage/src/lib.rs` (pagination tests)
- `crates/platform-windows/src/uia.rs` (UIA timeout)
- `crates/platform-windows/src/ocr_acquire.rs` (OCR timeout)
