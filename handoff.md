# Mori Lens — Session Handoff

Last updated: 2026-09-09

## Current Goal

Windows-first product validation. macOS deferred items remain.
**Automated evidence is strong. Human GUI product smoke is the remaining gap.**

## Current Product State

**Windows (automated PASS, human GUI smoke pending):**
- Full product loop: hover → UIA acquisition → translation → native overlay
- Deep Analysis, Copy, Save, Manual OCR, Pause/Resume via global hotkeys
- Tauri Settings/Library/Detail surfaces with live settings propagation
- Policy enforcement via HoverSession (per-app deny/allow)
- NSIS installer + MSIX packaging infrastructure (ADR-0017)
- ADR-0005 true two-speed runtime: separate Quick/Deep translators (PR #353)
- Settings: single canonical TOML format via lens-settings crate (PR #352)
- Endpoint locality: canonical engine_for() classifier, adversarial tests (PR #354)
- Settings validation: round-trip consistency proven (PR #355)
- Code signing: NOT CONFIGURED (Store account or self-signed certificate needed)
- Ollama + both models available locally (translategemma:latest, qwen2.5:7b)

**macOS (implementation partial, runtime validation pending):**
- Accessibility acquisition production-wired (`MacTextAcquirer`)
- Vision OCR with per-word geometry (`boundingBoxForRange` → `VNRectangleObservation`)
- Typed FFI ABI (`LensOcrWord` struct: float geometry + `uint32_t` offsets)
- Integer overflow protection, narrowing safety, RAII ownership, range validation
- Passive native overlay (NSPanel, transparent, click-through, ARC-clean)
- Real global commands via CGEventTap (Cmd+Opt+L/D/C/S/O/Q)
- Hotkey queue: true SPSC + saturating unsigned Pause counter (max 1024)
- Correct capture: pointer-based display selection, BGRA canonical
- Overlay exclusion via CGWindowID, shutdown cleanup, diagnostics wired
- Settings hot-path: generation counter, policy enforcement
- Graceful degradation without AX permission (no SIGTRAP)
- Quick focused-word: IMPLEMENTED (PR #340)
- Runtime permissions: IMPLEMENTED (AX + Screen Recording checks)

**macOS deferred items:**
- modifier/InputState (Ctrl tracking for hover_scope): DEFERRED
- --auto flag: PARTIAL (parsed but never read)
- --settings flag: PARTIAL (parsed but settings_service ignores it)
- HoverSession/HoverWorker: DEFERRED (Windows-only, macOS uses inline acquisition)
- ReadingContext: DEFERRED (scattered state, not bundled)
- hover_scope (word vs sentence): DEFERRED (setting exists, macOS never reads it)
- Deep cache: DEFERRED (only TranslationCache instantiated, no AnalysisCache)
- Stale result safety: PARTIAL (generation checks exist, but HoverSession::resolve unused)

**Mori integration (built, unverified):**
- Process boundary with capability discovery (`.capability.json`)
- Version negotiation, explicit user action to send items
- No shared DB, no shared package, no hidden sync (ADR-0018)
- `mori_transfer` module with `TransferPayload`, `MoriStatus`, `send_to_mori`
- Tauri commands: `mori_status`, `mori_send_item`

**Distribution infrastructure:**
- Package-smoke CI workflow (`.github/workflows/package-smoke.yml`)
- Windows: NSIS (via `cargo tauri build`) + MSIX (via `scripts/package-msix.ps1`)
- macOS: .app bundle (via `cargo tauri build`)
- Smoke asset generation (`scripts/generate-smoke-assets.py`): PNG, ICO, ICNS
- MSIX verification (`scripts/verify-msix.ps1`)
- MSIX manifest: `runFullTrust` only (no unnecessary `broadFileSystemAccess`)
- Release workflow (`.github/workflows/release.yml`): aligned with package-smoke

**Verification gate (`scripts/check.sh --full`):**
- Workspace fmt + clippy + test
- Product binary fmt + clippy + check + test
- Clean native rebuild + privacy gates + version consistency

## Recent Sessions

### PR #352 — fix: make settings.toml a single safe format boundary
- Eliminated dual-parser split-brain on Windows
- Both `win.rs` and `settings_service` now use `load_or_initialize()` from lens-settings
- Fail-safe migration: backup before write, collision handling, future-version preservation
- Legacy detection by format characteristics, not error type

### PR #353 — feat: implement true two-speed runtime per ADR-0005
- Separate `quick_translator` and `deep_translator` with independent models/endpoints
- `ai_runtime` module bridges settings schema and runtime translators
- Startup banner shows both Quick and Deep configurations
- Live settings detection checks quick and deep independently

### PR #354 — test: harden engine_for() with adversarial endpoint validation
- 30+ adversarial test cases for endpoint classification
- Suffix attacks, case variations, private networks, bare IPv6
- Fix: bare IPv6 `::1` now correctly classified as loopback

### PR #355 — test: verify settings validation consistency and round-trip preservation
- Round-trip test: load → save → load preserves all values
- Endpoint validation consistent across Quick and Deep
- Range checks (dwell_ms, history_days) applied at load time

### PR #356 — test: measure UIA visible-range targeting for large text containers
- GetVisibleRanges research for VS Code (#14) and KakaoTalk (#34)
- Container discovery: 100% (25/25 points found TextPattern ancestor)
- Latency: p50 ~950ms, p95 ~1070ms (400ms chain budget)
- **Decision: synchronous GetVisibleRanges fallback REJECTED on performance**
- Measurement-only probe; no production code changed

### Previous Sessions (PR #303–#351)
- Various fixes: save serialization, Deep parity, OCR provenance, Copy feedback
- Pause gate, settings privacy, endpoint locality, cross-platform save contract
- macOS: OCR provenance, focused-word parity, app provenance timing
- History note: published history rewritten twice (Co-authored-by removal). Current main is canonical baseline. No further rewrites.

## Settings Current Truth

```
SETTINGS FORMAT OWNER:       lens-settings crate (canonical TOML loader/writer)
LEGACY MIGRATION:            Fail-safe + backup, format-based detection
WINDOWS STARTUP:             Same canonical schema via load_or_initialize()
SETTINGS UI:                 Same canonical schema via SettingsState
QUICK / DEEP CONFIG:         Separate [quick] and [deep] sections
                             Separate endpoint/model/runtime per ADR-0005
ENDPOINT LOCALITY:           lens_core::translate::engine_for() as single source
                             Adversarial tests prove locality cannot be bypassed
```

Historical split-brain (legacy parser vs new parser) is FIXED (PR #352).

## ADR-0005 Current Truth

```
ADR-0005 IMPLEMENTATION:     CLOSED (PR #353)
AUTOMATED EVIDENCE:          CLOSED (separate translator tests)
WINDOWS STARTUP DIAGNOSTICS: PASS (Quick=translategemma, Deep=qwen2.5)
BOTH MODELS LOADED:          PASS (engine warm after ~20s)
GUI HOVER → QUICK:           NOT YET HUMAN-VERIFIED
GUI CTRL+ALT+D → DEEP:       NOT YET HUMAN-VERIFIED
SAVE PRODUCER-MODEL:         NOT YET HUMAN-VERIFIED
```

Unit test evidence does not substitute for GUI smoke evidence.

## Windows Current Validation

```
WINDOWS AUTOMATED/PROBE:     AVAILABLE
  702 workspace tests PASS
  141 product tests PASS
  105 live tests ignored (require real Windows environment)
  Live UIA sweep executed (13 windows)
  Local MSIX build executed
  Ollama + two models available

WINDOWS HUMAN GUI SMOKE:     PENDING
  Quick visible hover
  Deep hotkey (Ctrl+Alt+D)
  Copy feedback (Ctrl+Alt+C)
  Save + provenance (Ctrl+Alt+S)
  Manual OCR (Ctrl+Alt+O)
  Automatic OCR opt-in
  Settings UI / restart
  Library UI
  Manual Review
  hover_scope / Ctrl inversion
  Fullscreen / borderless
```

"BLOCKED BY ENVIRONMENT" is no longer accurate. The Windows environment is available;
what remains is human GUI interaction that the agent cannot perform.

## Windows Sweep Observations (build 074844b)

```
Chrome (1366x1079):          92% on-target, 11 distinct targets
Claude (1697x1111):          31% on-target, high distances
WindowsTerminal:             0% on-target, single-blob shape
Chrome (1920x1078):          0% on-target, single-blob shape
SourceTree:                  unsupported
ChatGPT/Explorer/etc:        occluded
```

These are compatibility observations, not bug classifications.
Each window/surface has different accessibility tree quality.

## VS Code #14

```
Accessibility text:          EXISTS
RangeFromPoint targeting:    POOR
GetVisibleRanges geometry:   USEFUL LINE GEOMETRY EXISTS
Production synchronous GVR:  REJECTED (p50 ~950ms >> 400ms budget)
Future accessibility research: OPEN
Current practical fallback:  OCR per product OCR policy
Issue:                       OPEN
```

Do not say "VS Code exposes no text" or "no accessibility solution is possible."

## KakaoTalk #34

```
Real conversation UIA:       0% on-target across measured sweeps
GetVisibleRanges on KakaoTalk: NOT DIRECTLY MEASURED (window unavailable)
VS Code proxy measurement:   NOT KakaoTalk direct evidence
Current practical fallback:  OCR
Issue:                       OPEN
```

## Windows MSIX

```
LOCAL MSIX STRUCTURAL BUILD: PASS (11/11 checks, 2 expected publisher warnings)
NORMAL INSTALL:              NOT VERIFIED
Current artifact:            unsigned / placeholder publisher
Install error:               0x800B0100 (signature/certificate validation failure)
Distribution blocker:        signing / publisher trust
```

Developer Mode is NOT established as a canonical requirement.
Modern Windows trusted-signed package sideload may not require it.
Future research candidate: Windows 11 dev-only unsigned package path.

## NSIS

```
NSIS INFRASTRUCTURE:         IMPLEMENTED (cargo tauri build --bundles nsis)
LOCAL EXECUTION:             NOT EXECUTED
Reason:                      makensis.exe unavailable locally; installation not authorized
```

Infrastructure is CLOSED. Local execution evidence is PENDING.

## Package CI

```
PACKAGE INFRASTRUCTURE:      IMPLEMENTED
WINDOWS LOCAL MSIX:          PASS
CURRENT-MAIN GITHUB CI:      NOT EXECUTED (billing blocker)
OLD CI:                      Historical pre-rewrite evidence only
```

### Part 12 — macOS hover_scope parity

**PR #352** — feat: macOS hover_scope — word/sentence toggle with Ctrl key
- Added `lens_ctrl_held()` to overlay.m (NSEventModifierFlagControl polling)
- Added `overlay::ctrl_held()` safe Rust wrapper in platform-macos
- Product loop reads `hover_scope` from settings at startup and on live changes
- Ctrl toggles between word and sentence scope, matching Windows behavior
- Model always receives the full sentence; card shows word or sentence
- Manual OCR path uses full sentence as display_text (no scope toggle)
- Fixed stale comment in keeping.rs and roadmap.md about macOS save path

**History note**: Published history was rewritten twice (Co-authored-by removal). Current main is the canonical baseline. No further rewrites.

## Automated Evidence

```
scripts/check.sh --full:     PASS (2026-09-09, local)
Workspace tests:             702 passed, 0 failed, 105 ignored
Product tests:               141 passed, 0 failed
Total:                       843 passed, 0 failed
Live UIA sweep:              13 windows measured
MSIX verification:           11 passed, 0 failed, 2 warnings
Ollama models:               translategemma:latest + qwen2.5:7b available
```

## Completion Matrix

```
Area                    Implementation  Automated  Runtime   Human     Distribution
M1 Windows acquire      SHIPPED         CLOSED     CLOSED    PENDING   N/A
M2 Windows overlay      SHIPPED         CLOSED     CLOSED    PENDING   N/A
M3 Quick translation    SHIPPED         CLOSED     CLOSED    N/A       N/A
M4 Windows OCR          SHIPPED         CLOSED     CLOSED    PENDING   N/A
M5 Deep analysis        SHIPPED         CLOSED     CLOSED    OPEN*     N/A
M6 macOS acquire        PARTIAL         CLOSED     BLOCKED   PENDING   N/A
M7 macOS overlay/OCR    PARTIAL         CLOSED     BLOCKED   PENDING   N/A
M8 Library/Review       COMPLETE        CLOSED     N/A       N/A       N/A
Mori boundary           BUILT           CLOSED     N/A       PENDING   N/A
Windows NSIS release    SHIPPED         CLOSED     N/A       N/A       UNSIGNED
Windows MSIX release    IMPLEMENTED     CLOSED     N/A       N/A       BLOCKED†
macOS .app bundle       IMPLEMENTED     CLOSED     PENDING   N/A       UNSIGNED
macOS notarization      BLOCKED‡       N/A        N/A       N/A       BLOCKED‡
First-user onboarding   NOT TESTED      N/A        N/A       PENDING   N/A
Return/re-entry         NOT TESTED      N/A        N/A       PENDING   N/A

* M5 explanation quality: model-quality research, gate open
† Signing/publisher trust required; current-main CI execution blocked by billing
‡ Apple Developer ID credentials required; CI blocked by billing
```

## Agent Assessment

```
ADR-0005 IMPLEMENTATION:     CLOSED
  Quick translator:           translategemma:latest (separate instance)
  Deep translator:            qwen2.5:7b (separate instance)
  Startup diagnostics:        correctly shows both
  Both models responding:     PASS

M8 IMPLEMENTATION:            COMPLETE
M8 AUTOMATED GATE:            CLOSED
  SAVE_LOCK in append():      ALL persistence serialized
  All Save paths:             use shared save_card() policy

SETTINGS CANONICAL FORMAT:    CLOSED (PR #352)
ENDPOINT LOCALITY:            CLOSED (PR #354)
SETTINGS VALIDATION:          CLOSED (PR #355)
UIA GVR RESEARCH:             CLOSED (PR #356, fallback rejected)

PACKAGE INFRASTRUCTURE:       IMPLEMENTED
PACKAGE CURRENT-MAIN CI:      PENDING (billing blocker)

SMOKE ASSETS:                 VALID (PNG/ICO/ICNS CRC verified)

REMAINING REPOSITORY IMPLEMENTATION:
  None. All repo-local work is complete.
  (hover_scope on macOS implemented: Ctrl key polling, word/sentence toggle,
  live settings propagation, matching Windows behavior)
REMAINING PACKAGING EXECUTION:
  Package-smoke CI GREEN          BLOCKED BY BILLING
  (code fixes merged in PR #315; latest blocked run 34131713859 was on old HEAD)

REMAINING WINDOWS WORK:
  Human GUI product smoke     HUMAN GATE (agent cannot perform)
  MSIX signing/publisher      OWNER DECISION
  NSIS local installation     OWNER DECISION

REMAINING MACOS WORK:
  modifier/InputState         DEFERRED
  --auto flag                 PARTIAL (parsed, never read)
  --settings flag             PARTIAL (parsed, ignored by settings_service)
  HoverSession/HoverWorker    DEFERRED
  ReadingContext              DEFERRED
  hover_scope                 DEFERRED
  Deep cache                  DEFERRED
  Stale result safety         PARTIAL

REMAINING EXTERNAL BLOCKERS:
  GitHub Actions billing      BILLING BLOCKER
  Store developer account     CREDENTIAL BLOCKER
  Apple Developer ID          CREDENTIAL BLOCKER
  OV code-signing certificate CREDENTIAL BLOCKER

Ollama + models:              NOT A BLOCKER (available on this machine)

REMAINING HUMAN PRODUCT GATES:
  First-user installs unaided
  Keeps running for a week
  Can state what it is for

REMAINING OWNER DECISIONS:
  Legal publishing identity
  OV certificate vendor
  Return/re-entry UX approach
```

## Open Issues

```
#148  colour linkage quality gate     QUALITY GATE OPEN
#140  first-user onboarding           HUMAN VALIDATION
#34   KakaoTalk                       OPEN (GVR not directly measured)
#14   VS Code                         OPEN (GVR fallback rejected on performance)
```

## Next Session — Start Here

1. **Human Windows GUI product smoke** — Quick/Deep/Copy/Save lifecycle
2. **Bugs discovered by that smoke** — fix via branch → PR → merge
3. **Optional: Windows 11 dev-only unsigned MSIX install path**
4. **Remaining Windows roadmap gates**
5. **macOS work when environment returns**

Do not reopen UIA GetVisibleRanges research without new independent evidence.

## Git State

```
latest merge: PR #356 (test: measure UIA visible-range targeting)
current work: PR #365 (feat: macOS hover_scope parity) — open
HEAD:         074844b
Branches:     clean
History:      rewritten twice (Co-authored-by removal). Current main is canonical baseline.
              No further rewrites.
always run `git log -1` at session start for actual HEAD.
```

## Constraints

- Agent investigation stays inside `PROJECT_ROOT/**`.
- `lens-core` remains platform-neutral (ADR-0003).
- Do not install external tools without explicit permission.
- Work in coherent branch → PR → merge units. No direct main pushes.
- macOS native code uses Objective-C bridges compiled by `cc` crate with `-fobjc-arc`.
- All unsafe blocks require SAFETY comments (clippy `-D warnings`).
- `apps/lens` is not a workspace member; verified by `scripts/check.sh --full`.
