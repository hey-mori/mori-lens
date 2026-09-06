# Mori Lens — Session Handoff

Last updated: 2026-09-06

## Current Goal

macOS native implementation for Mori Lens cross-platform desktop product.
Windows implementation is complete and stable. macOS implementation is complete.
**All repository-local implementation work is complete. Remaining work is validation, external accounts, human gates, or quality research.**

## Current Product State

**Windows (stable, production):**
- Full product loop: hover → UIA acquisition → translation → native overlay
- Deep Analysis, Copy, Save, Manual OCR, Pause/Resume via global hotkeys
- Tauri Settings/Library/Detail surfaces with live settings propagation
- Policy enforcement via HoverSession (per-app deny/allow)
- NSIS installer + MSIX packaging infrastructure (ADR-0017)
- Code signing: NOT CONFIGURED (Store account needed for MSIX path)

**macOS (implementation complete, CI passing, real-world validation pending permissions):**
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

**Verification gate (`scripts/check.sh --full`):**
- Workspace fmt + clippy + test
- Product binary fmt + clippy + check + test
- Clean native rebuild + privacy gates + version consistency

## This Session

- **PR #303** — docs: fix stale macOS claims in project-goal.md
- **PR #304** — docs: add M6/M7/M8 gate status tables to roadmap
- **PR #305** — feat: Windows MSIX packaging infrastructure (ADR-0017)
- **PR #306** — docs: fix settings.conf → settings.toml, README milestone
- **PR #307** — fix: macOS graceful degradation without AX permission (no more SIGTRAP)

## Completion Matrix

```
Area                    Implementation  Automated  Runtime   Human     Distribution
M1 Windows acquire      SHIPPED         CLOSED     CLOSED    PENDING   N/A
M2 Windows overlay      SHIPPED         CLOSED     CLOSED    PENDING   N/A
M3 Quick translation    SHIPPED         CLOSED     CLOSED    N/A       N/A
M4 Windows OCR          SHIPPED         CLOSED     CLOSED    PENDING   N/A
M5 Deep analysis        SHIPPED         CLOSED     CLOSED    OPEN*     N/A
M6 macOS acquire        COMPLETE        CLOSED     BLOCKED   PENDING   N/A
M7 macOS overlay/OCR    COMPLETE        CLOSED     BLOCKED   PENDING   N/A
M8 Library/Review       COMPLETE        CLOSED     N/A       N/A       N/A
Mori boundary           PLANNED         N/A        N/A       N/A       N/A
Windows NSIS release    SHIPPED         CLOSED     N/A       N/A       UNSIGNED
Windows MSIX release    INFRASTRUCTURE  CLOSED     N/A       N/A       BLOCKED†
macOS raw binary        BUILT           CLOSED     N/A       N/A       UNSIGNED
macOS .app bundle       BLOCKED‡       N/A        N/A       N/A       BLOCKED‡
First-user onboarding   NOT TESTED      N/A        N/A       PENDING   N/A
Return/re-entry         NOT TESTED      N/A        N/A       PENDING   N/A

* M5 explanation quality: model-quality research, gate open (P1 lemma 63%)
† Store developer account required (external)
‡ cargo-tauri not installed (tooling blocker)
```

## Agent Assessment

```
REMAINING CODE IMPLEMENTATION:    NONE
  (all repository-local implementation work is complete)

REMAINING ROADMAP GATES:
  M1 correct-word human judgement     HUMAN VALIDATION
  M2 exclusive fullscreen             HUMAN/HARDWARE VALIDATION
  M4 scanned PDF/image-text           HUMAN VALIDATION
  M4 VS Code/KakaoTalk OCR            HUMAN VALIDATION
  M5 explanation quality              QUALITY RESEARCH (model-dependent)
  M6 macOS compatibility matrix       HUMAN VALIDATION (needs AX permission)
  M7 macOS overlay/OCR real-world     HUMAN VALIDATION (needs Screen Recording)
  M8 gate                             CLOSED (automated evidence sufficient)

REMAINING QUALITY GATES:
  #148 colour linkage                 QUALITY GATE OPEN (threshold not met)
  M5 explanations right               QUALITY RESEARCH OPEN (model-dependent)

REMAINING PRODUCT GATES:
  First-user installs unaided         HUMAN PRODUCT GATE
  Keeps running for a week            HUMAN PRODUCT GATE
  Can state what it is for            HUMAN PRODUCT GATE

REMAINING PLATFORM VALIDATION:
  macOS AX/OCR/overlay/hotkeys        BLOCKED BY PERMISSIONS
  macOS sleep/wake/hot-plug           BLOCKED BY HARDWARE
  Windows VS Code/KakaoTalk           BLOCKED BY ENVIRONMENT

REMAINING DISTRIBUTION WORK:
  MSIX packaging infrastructure       IMPLEMENTED (this session)
  Store submission                    BLOCKED (developer account)
  macOS .app bundle                   BLOCKED (cargo-tauri not installed)
  Code signing (Windows direct)       BLOCKED (OV certificate)
  Code signing (macOS)                BLOCKED (Developer ID)
  Notarization                        BLOCKED (Apple account)
  Real icon assets                    ASSET BLOCKER

REMAINING EXTERNAL BLOCKERS:
  Store developer account             CREDENTIAL BLOCKER
  Apple Developer ID                  CREDENTIAL BLOCKER
  OV code-signing certificate         CREDENTIAL BLOCKER
  cargo-tauri CLI                     TOOLING BLOCKER
  Ollama + model (Quick/Deep test)    SERVICE BLOCKER

REMAINING OWNER DECISIONS:
  Legal publishing identity           OWNER DECISION
  OV certificate vendor               OWNER DECISION
  Return/re-entry UX approach         OWNER DECISION
```

## Automated Evidence

```
scripts/check.sh --full:          PASS (2026-09-06)
workspace fmt/clippy/tests:       PASS (665+ tests)
product fmt/clippy/check/test:    PASS
clean native rebuild:             PASS
platform-macos tests:             PASS (56 tests including 16 hotkey)
lens-translate tests:             PASS (8 HTTP fault tests)
lens-storage tests:               PASS (73 tests, full M8 lifecycle)
privacy static gate:              CLEAN
CI run 34027544715:               PASS (both platforms)
```

## Known Runtime Issues

- Lock file permission bug (`--w-------` → fixed to `-rw-------`)
- Ollama not available locally (Quick/Deep translation BLOCKED)
- macOS binary starts gracefully without AX permission (SIGTRAP fixed in PR #307)

## Validation Procedure

When permissions are granted, run:

```bash
bash scripts/validate-macos.sh
```

This script handles: permission check → Safari fixture → binary start → evidence recording → visual checkpoints → cleanup.

## Controlled Fixture

`docs/fixtures/validation-surface.html` contains deterministic test content: English, Korean, Japanese, mixed language, repeated words, Unicode/emoji.

## Open Issues

```
#148  colour linkage quality gate     QUALITY GATE OPEN
#140  first-user onboarding           HUMAN VALIDATION
#34   KakaoTalk                       WINDOWS VALIDATION (area:acquire)
#14   VS Code                         WINDOWS VALIDATION (area:platform-windows)
```

Note: #14 and #34 are Windows-specific (UIA control types, platform-windows label).

## Next Session — Start Here

1. Grant Accessibility + Screen Recording permissions
2. (Optional) Start Ollama with a model
3. Run `bash scripts/validate-macos.sh`
4. Review evidence in `docs/benchmarks/`

## Git State

```
latest merge: PR #305 (MSIX packaging infrastructure)
CI pass: run 34027544715 (both platforms)
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
