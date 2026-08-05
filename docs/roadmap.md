# Roadmap

Milestones are gates, not a schedule. Each one names what must be true
before the next starts, and the gate is evidence, not agreement that it
is probably fine.

The ordering principle: **the riskiest unknown goes first.** Text
acquisition is the only part of Lens that could turn out to be
impossible, so it is measured before anything is built on top of it
([ADR-0008](decisions/ADR-0008-probe-before-product.md)).

## M0 · Repository and decision baseline

Independent repository, workflow and commit contract, the ADRs, the
research brief, the compatibility and benchmark formats, a workspace
that builds, a verification script.

**Gate** · `scripts/check.sh` passes on an empty-ish workspace, no Mori
dependency exists, and the decisions that constrain M1 are written down.

## M1 · Windows hover probe

The measurement instrument. Pointer sampling, hover intent, UI
Automation acquisition under the pointer, word / sentence / paragraph
segmentation, screen rectangles, per-stage latency, typed failure
reasons, and a compatibility log. No translation, no overlay, no model.

**Gate** · a filled-in
[`compatibility/windows-app-matrix.md`](compatibility/windows-app-matrix.md)
covering at least: Notepad, Chrome, Edge, VS Code, Word, Edge's PDF
viewer, Windows Settings, and one Electron messenger. Each with the
acquisition result, the correct-word judgement, and latency. Plus: no
runaway UI Automation calls during fast pointer movement, no crash from
an element belonging to a closed window, and behaviour recorded at more
than one DPI scale and on more than one monitor.

Standing, 2026-08-05:

| Condition | State |
|---|---|
| Notepad, Chrome, Edge, VS Code, Word, Electron messenger | measured |
| Windows Settings, Edge PDF viewer | measured 2026-08-05; Edge on HTML measured too, which the earlier run had not been |
| correct-word judgement by a person | **open**; the on-target rate is a proxy and no human has confirmed a resolved word |
| no runaway calls on fast movement | measured: a pointer crossing a monitor costs zero calls, a resting hand costs one |
| no crash from a closed window's element | measured: a typed error in 153ms, and twenty further reads survive |
| more than one DPI scale, more than one monitor | measured |

## M2 · Windows native overlay

Passive tooltip that never takes focus and passes clicks through; the
interactive card; placement as a pure function; tray icon and pause;
per-application tiers.

**Gate** · typing focus is never stolen, verified against a text editor;
fast pointer movement produces no card spray; Lens never reads its own
overlay; verified on multiple monitors and in a full-screen application.

## M3 · Quick translation

Ollama discovery and health, model routing over the discovered set,
script and language detection, the quick prompt, memory cache, stale
result discard, loading states, target language setting.

**Gate** · Ollama absent degrades rather than fails; warm and cold
latency are measured and recorded in
[`benchmarks/`](benchmarks/); a repeat hover is served from cache; a card
never appears attached to the previous target; source text already in the
target language is skipped.

## M4 · Deep analysis

The second path: clause structure, grammar, lemma and part of speech,
idiom, developer and exam profiles. Schema-constrained output, copy, and
save.

**Gate** · quick-path latency is unchanged, proven by re-running the M3
benchmark; the analysis model is never called without a user action; a
schema violation degrades instead of crashing; output is labelled as an
AI explanation.

## M5 · Windows manual OCR

The fallback: modifier-triggered small-crop capture, an OCR backend
behind a trait, word polygons, pointer-to-word resolution, and the
overlay exclusion.

**Gate** · OCR never runs unasked; a scanned PDF and an image-text case
both work; Lens never recognises its own overlay; no captured image
reaches disk.

## M6 · macOS acquisition

The AX adapter behind the same `TextAcquirer` trait: point hit test,
character range, string for range, bounds for range, permission flow,
Retina and Spaces coordinate handling.

**Gate** · a macOS compatibility matrix comparable to M1's; `lens-core`
unchanged by the addition, which is the test of whether
[ADR-0003](decisions/ADR-0003-rust-core-platform-adapters.md) held.

## M7 · macOS overlay and Vision OCR

`NSPanel` overlay, accessibility and screen-recording onboarding staged
separately, ScreenCaptureKit crops, Vision recognition.

**Gate** · a user who never uses OCR is never asked for screen
recording; Spaces and full-screen verified; Retina coordinates match.

## M8 · Learning

Saved words and sentences, notes, review list, export. Saving is always
explicit; nothing is captured automatically.

**Gate** · nothing is stored without a user action; export round-trips;
deletion is real.

## After M8 · Mori integration

A process boundary carrying learning items on an explicit user action.
Never a shared library, never a read of Mori's database
([ADR-0006](decisions/ADR-0006-no-shared-package-yet.md)).

## Not on this roadmap

Automatic full-screen translation, a browser extension, cloud sync,
accounts, teams, flashcard generation, bundled models, fine-tuning, and
live video subtitle translation. Each is excluded on purpose; adding one
is a decision, not a task.
