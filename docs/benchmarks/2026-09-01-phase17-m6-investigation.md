# Phase 17 — M6 macOS Acquisition Investigation

Date: 2026-09-01

## Objective

Prove that the `TextAcquirer` seam validated on Windows actually holds on macOS Accessibility API. Verify ADR-0003's core promise:

```text
lens-core
    ↑ trait
platform-windows
platform-macos
```

This Phase does **not** build a macOS product. It builds a measurement instrument and records whether the architecture survives contact with a second platform.

## Step 0 — Phase 15/16 Cleanup ✓

Committed Phase 15/16 changes:
- Deterministic answerability projection (`only_answerable()`)
- No-word analysis NOT suppressed
- Sentence-level structure remains available
- Unanswerable word fields removed
- Raw model quality and product-visible quality remain separate
- Phase 14's incorrect proposal documented as superseded

Commit: `4dc6cfa`

## Step 1 — M6 Contract Audit ✓

### Core Seam (lens-core)

macOS adapter must implement:

#### TextAcquirer trait (acquire.rs:453-459)
```rust
pub trait TextAcquirer {
    fn acquire(
        &self,
        point: ScreenPoint,
        context: &AcquireContext,
    ) -> Result<TextTarget, AcquireError>;
}
```

#### TextTarget struct (acquire.rs:249-259)
```rust
pub struct TextTarget {
    pub app: AppIdentity,           // process name, window title
    pub source: AcquisitionSource,  // Accessibility, Ocr, or Lesson
    pub provider: ProviderInfo,     // control_id, woke_accessibility, expanded_unit
    pub segmented: SegmentedText,   // context, focus, word, sentence, paragraph
    pub word_bounds: Vec<ScreenRect>,
    pub context_bounds: Vec<ScreenRect>,
}
```

#### AcquireError enum (acquire.rs:82-109)
- `Unsupported` - provider exposes no text interface
- `Empty` - read succeeded and returned nothing
- `KnownEmpty` - previous reading showed no line on this pixel
- `ElementGone` - element vanished mid-call
- `Protected(ProtectedReason)` - password or protected field
- `Timeout` - application did not answer in time
- `Stale` - pointer moved before finished
- `Unavailable` - accessibility layer unreachable (permission/policy)

#### ProtectedReason enum (acquire.rs:70-78)
- `Declared` - provider said so (password box)
- `Unprovable` - check itself failed, unprovable treated as secure

#### ScreenPoint struct (geometry.rs:14-18)
```rust
pub struct ScreenPoint {
    pub x: i32,  // physical screen pixels
    pub y: i32,
}
```

#### ScreenRect struct (geometry.rs:38-44)
```rust
pub struct ScreenRect {
    pub x: i32,      // physical screen pixels
    pub y: i32,
    pub width: i32,
    pub height: i32,
}
```

#### SegmentedText struct (text.rs:57-64)
```rust
pub struct SegmentedText {
    context: String,        // full text
    focus: usize,           // byte offset pointer resolved to
    word: Option<TextSpan>, // word range
    sentence: TextSpan,     // sentence range
    paragraph: TextSpan,    // paragraph range
}
```

#### AcquireContext struct (acquire.rs:411-434)
```rust
pub struct AcquireContext {
    pub generation: Generation,
    pub max_context_chars: usize,
    pub allow_accessibility_wake: bool,
    pub view_may_have_changed: bool,
}
```

### Key Observations

1. **All types are platform-neutral** - no Windows-specific concepts in core
2. **Coordinates are physical screen pixels** - signed, to handle negative monitor positions
3. **Failure taxonomy is closed** - 8 variants, each with distinct semantic meaning
4. **Protected field check is mandatory** - must refuse before reading any text
5. **Segmentation is core's job** - adapter provides raw text + focus offset, core segments

### Contract Summary

macOS adapter must:
1. Accept `ScreenPoint` in physical screen pixels
2. Return `TextTarget` with all fields populated
3. Map platform failures to `AcquireError` variants
4. Check protected fields before reading
5. Provide word/context bounds as `Vec<ScreenRect>`
6. Handle accessibility wake (Chromium/Electron)
7. Respect timeouts and budgets

## Step 2 — Windows Adapter Implementation Pattern ✓

### Flow

```text
ScreenPoint
    ↓
POINT (native)
    ↓
ElementFromPoint → IUIAutomationElement
    ↓
CurrentIsPassword → Protected check
    ↓
GetCurrentPattern → IUIAutomationTextPattern
    ↓
RangeFromPoint → hit range (degenerate)
    ↓
ExpandToEnclosingUnit(Word) → word range
ExpandToEnclosingUnit(Paragraph/Line) → context range
    ↓
GetText → text
    ↓
focus_offset → pointer byte offset in context
    ↓
bounding_rectangles → Vec<ScreenRect>
    ↓
text::segment → SegmentedText
    ↓
TextTarget
```

### Key Implementation Details

1. **DPI awareness** - declared before creating automation client
2. **COM apartment** - acquirer bound to thread, not Send
3. **Timeouts** - per-call (800ms) + per-chain (400ms) + connection (300ms)
4. **Wake mechanism** - throttled by window + time, not once-per-session
5. **Password check** - before any text is read, Unprovable treated as secure
6. **Focus offset** - computed from degenerate hit range, not expanded word
7. **Bounds** - GetBoundingRectangles returns physical screen pixels

### What macOS Must Replicate

1. Point → element resolution
2. Password/protected check before reading
3. Text pattern/range API
4. Range expansion (word, paragraph/line)
5. Text extraction with character cap
6. Bounding rectangle extraction
7. Focus offset computation
8. SegmentedText construction
9. Timeout/budget enforcement
10. Accessibility wake (for Chromium/Electron)

## Step 3 — M6 Gate Reconstruction

### Current Roadmap Statement

```text
M6 · macOS acquisition

Gate:
- macOS compatibility matrix comparable to M1
- lens-core unchanged
```

### Verifiable Sub-conditions

1. **Pointer hit-test works**
   - `AXUIElementCopyElementAtPosition` returns element
   - Element can be navigated to text-bearing child

2. **Word resolution works**
   - Range-for-position API resolves to character offset
   - Word expansion API returns word or normalizes up

3. **Sentence/range resolution works**
   - Paragraph/line expansion API works
   - Text extraction with character cap works

4. **Screen bounds correspond to visible text**
   - Bounds-for-range API returns rectangles
   - Rectangles are in physical screen pixels
   - Rectangles match what is visible on screen

5. **Protected/secure text is refused**
   - Password field detection works
   - Refusal happens before text is read
   - Typed as `Protected(Declared)` or `Protected(Unprovable)`

6. **Permission denial is typed, not crash**
   - AX trust state check exists
   - Missing permission returns `Unavailable`
   - No panic or undefined behavior

7. **Stale/closed element does not crash**
   - Element vanishing mid-call returns `ElementGone`
   - No dangling references or use-after-free

8. **Retina coordinate conversion works**
   - AX coordinates (points) converted to physical pixels
   - Multi-monitor coordinates handled correctly
   - Negative coordinates (left/above primary) work

9. **Multiple displays work**
   - Secondary monitor coordinates resolve correctly
   - Bounds are in global screen space

10. **Core requires no macOS-specific changes**
    - `lens-core` compiles unchanged on macOS
    - No `#[cfg(target_os = "macos")]` added to core
    - All platform-specific code in `platform-macos`

### Measurement Categories

**Gate conditions** (must all pass):
- 1-10 above

**Supporting measurements** (inform, do not gate):
- Latency per app
- Wake mechanism effectiveness
- Over-expansion frequency
- Empty read rate

## Next Steps

### Step 4 — macOS API Research

Investigate actual AX APIs:
- `AXUIElementCreateSystemWide`
- `AXUIElementCopyElementAtPosition`
- `AXUIElementCopyParameterizedAttributeValue`
- `AXUIElementCopyAttributeValue`
- `AXValue / CFRange`
- Attribute names: `kAXStringForRangeParameterizedAttribute`, etc.

Classify each as:
- `documented` - Apple docs confirm
- `observed` - seen in other projects
- `unknown` - needs empirical verification

### Step 5 — Permission Model Investigation

macOS Accessibility permission lifecycle:
- Not granted → requested → granted → revoked while running

Key questions:
- How to check trust state?
- How to guide user to System Settings?
- What typed failure when permission missing?
- Is onboarding UI needed now or later?

### Step 6 — Coordinate System Investigation

macOS coordinate systems:
- CGEvent / mouse coordinates
- AX coordinates
- AppKit coordinates
- CoreGraphics display coordinates
- Retina backing scale
- Multiple monitors

Key questions:
- Top-left vs bottom-left origin?
- Logical point vs backing pixel?
- Negative monitor coordinates?
- Retina scale factor?

### Step 7 — Swift Bridge Decision

Compare:
A. Rust → Objective-C/CoreFoundation direct FFI
B. Rust → small Swift bridge → AX APIs
C. Existing maintained Rust bindings

Criteria:
- Memory ownership
- CF lifetime safety
- API coverage
- Build complexity
- Testability
- Future AppKit/ScreenCaptureKit reuse

Default: small Swift bridge (ADR-0003) unless evidence says otherwise.

## Stop Condition

This Phase succeeds if:
- `platform-macos` skeleton exists
- AX trust state check works
- Pointer → element works
- At least one real app returns text
- Bounds obtained
- Typed result produced
- `lens-core` unchanged

Failure is also valid evidence. If Safari works but VS Code cannot resolve point to range, that is a result, not a blocker.

## Absolute Constraints

This Phase does NOT:
- Build macOS overlay
- Add ScreenCaptureKit
- Build Vision OCR
- Build tray/menu bar UX
- Build settings UI
- Modify translation/model path
- Add `#[cfg(target_os = "macos")]` to lens-core
- Break Windows adapter

These are M7 or later.
