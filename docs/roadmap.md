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
| correct-word judgement by a person | **open**, and narrowed. A second instrument now reads the same pixel: on one window, 29 points where both accessibility and OCR resolved a word, they agreed on 72.4%, overlapped on 10.3%, and **differed on 17.2%**. Two methods agreeing is not a person confirming, but the human pass is now the disagreements rather than everything |
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

Standing, 2026-08-05:

| Condition | State |
|---|---|
| focus is never stolen | measured: the foreground window is unchanged across a show |
| no card spray on fast movement | measured: a pointer crossing a monitor draws zero cards, and six queued answers draw one |
| Lens never reads its own overlay | measured: 193 characters read identically with a card drawn over that exact point |
| more than one monitor | measured: a card anchored on the second monitor stays on it |
| a full-screen application | measured 2026-08-18: a card is in front of a borderless full-screen window at z 13 against 38, and of a topmost one at 13 against 18, with the foreground window unchanged. **Exclusive full screen is not covered** and still needs a game and a person ([`a card over a full-screen window`](benchmarks/2026-08-18-a-card-over-a-full-screen-window.md)) |
| tray icon | built and measured: the shell accepts it, a click becomes a pause, and it leaves on exit |
| per-application tiers | built: set in the settings file, with the password-manager floor unreachable by omission |

## M3 · Quick translation

Ollama discovery and health, model routing over the discovered set,
script and language detection, the quick prompt, memory cache, stale
result discard, loading states, target language setting.

**Gate** · Ollama absent degrades rather than fails; warm and cold
latency are measured and recorded in
[`benchmarks/`](benchmarks/); a repeat hover is served from cache; a card
never appears attached to the previous target; source text already in the
target language is skipped.

## M4 · Windows manual OCR

**Swapped with deep analysis on 2026-08-05.** The reason is in this
repository's own measurements, not in a preference:

| Application | Acquired | What that means |
|---|---|---|
| VS Code ([#14](https://github.com/swarea/mori-lens/issues/14)) | 7 of 247 points, all `50020` labels | a pointer reaches almost none of the editing surface |
| KakaoTalk ([#34](https://github.com/swarea/mori-lens/issues/34)) | 14 of 104, **1 separable target** | one blob, no way to aim inside it |

**That first row said "the editing surface exposes nothing" until
2026-08-07, and it was wrong.** VS Code's document *is* in the
accessibility tree: six words of one doc comment were found in it by
handle, and the tree holds three times as many distinct words as the
screen shows. What it does not do is hand a **point** to a sentence,
because the text is in elements too large to aim inside
([`benchmarks/2026-08-07-vs-code-with-a-screen-reader.md`](benchmarks/2026-08-07-vs-code-with-a-screen-reader.md)).

The swap stands, and for a better reason than the one it was made on.
OCR is still what reads that window, and the two rows turn out to be the
**same** problem rather than two: a blob you cannot aim inside, in an
editor and in a messenger.

Those are a code editor and a messenger: the two surfaces the first user
spends the day in. Deep analysis explains a sentence Lens has already
read, so building it first would deepen the product exactly where it
already works and leave it blind where it does not.

**Reach before depth.** Nothing in
[ADR-0002](decisions/ADR-0002-accessibility-first-acquisition.md) is
reversed by this: OCR remains the fallback, tried only after the
accessibility path has failed or when the user asks. It arrives sooner,
which is what that ADR always intended it for.

The fallback: modifier-triggered small-crop capture, an OCR backend
behind a trait, word polygons, pointer-to-word resolution, and the
overlay exclusion.

**Gate** · OCR never runs unasked; a scanned PDF and an image-text case
both work; Lens never recognises its own overlay; no captured image
reaches disk; and VS Code and KakaoTalk, the two applications that
prompted the swap, are measured again with it.

**IMPLEMENTATION: complete.** All OCR code paths, the overlay exclusion
guard, the capture pipeline, and the contract tests are merged and
verified by automated tests. What remains is real-world measurement on
actual desktop surfaces, which requires a human operator and is tracked
as validation debt.

Standing, 2026-09-03:

| Condition | Automated | Real-world |
|---|---|---|
| OCR never runs unasked | CLOSED (4 contract tests) | N/A |
| a scanned PDF and an image-text case | CLOSED (synthetic 1.0x, 82-86%) | **AWAITING** real viewer measurement |
| Lens never recognises its own overlay | CLOSED (contract + OCR test) | **AWAITING** real compositor timing |
| no captured image reaches disk | CLOSED (clippy + test) | N/A |
| VS Code measured again | CLOSED (2026-08-06, 93 lines) | **AWAITING** fresh measurement |
| KakaoTalk measured again | CLOSED (accessibility only) | **AWAITING** fresh OCR measurement |

**"Unasked" means as installed, and that is the whole of it.** As of
2026-08-06 `ocr = automatic` exists, and under it a hover the
accessibility path could not serve falls through to pixels with no
keypress. That is what
[ADR-0002](decisions/ADR-0002-accessibility-first-acquisition.md)
describes rather than a departure from it, and it is what an editor or a
messenger needs to be readable by hovering. The default is `manual` and
stays there: a tool that photographs the screen on its own has to be
turned on by somebody. A reader who writes that line in their settings
file has asked.

**Answered, 2026-08-05** ([#62](https://github.com/swarea/mori-lens/issues/62)).
`Windows.Media.Ocr` documents a package identity requirement for desktop
applications, and Lens ships as a single unpackaged `lens.exe`. Measured
from a binary built the same way:

```
engine available: true
languages: ["en-US", "ko"]
recognised:   "The output files came back empty."
korean text:  "출력 파일이 비어 있었습니다."
```

Both exact, both from a bitmap this repository drew rather than
captured. The backend is viable and costs the delivery model nothing.

What that does **not** settle is accuracy on real screen content: small
text, dark themes, and the code editor that prompted the swap. That is
milestone work, and it needs the capture path this spike deliberately
avoided.

## M5 · Deep analysis

The second path: clause structure, grammar, lemma and part of speech,
idiom, developer and exam profiles. Schema-constrained output, copy, and
save.

**Gate** · quick-path latency is unchanged, proven by re-running the M3
benchmark; the analysis model is never called without a user action; a
schema violation degrades instead of crashing; output is labelled as an
AI explanation.

Standing, 2026-08-06, updated 2026-09-01. The model side is built and
measured; the product calls it; explanation quality is measured.

| Condition | State |
|---|---|
| quick-path latency unchanged | measured: 1,607ms before an analysis and 1,668ms after, sharing one process, one endpoint and one model lease |
| never called without a user action | held by construction: `lens-core::session` has no reference to an `Analyser` and there is no path from a settled hover to one. The only caller is a `Ctrl+Alt+D` press |
| a schema violation degrades | built: a missing field is an empty field, an answer that was not JSON says nothing, and an analysis that says nothing is a typed error rather than a card of headings over blanks |
| output labelled as an AI explanation | built and shown: `DeepAnalysis::LABEL`, first in the footer because a card cuts from the end |
| the explanations are right | **open**, measured on 2026-09-01 ([`deep analysis evaluation`](benchmarks/2026-09-01-deep-analysis-evaluation.md), [`prompt A/B`](benchmarks/2026-09-01-prompt-ab-comparison.md), [`counterpart analysis`](benchmarks/2026-09-01-counterpart-ab-analysis.md), [`P1 full corpus validation`](benchmarks/2026-09-01-p1-full-corpus-validation.md), [`evaluation integrity audit`](benchmarks/2026-09-01-evaluation-integrity-audit.md), [`P3 prompt evaluation`](benchmarks/2026-09-01-p3-prompt-evaluation.md), [`Phase 13 final report`](benchmarks/2026-09-01-m5-phase13-final-report.md), [`Phase 14 final report`](benchmarks/2026-09-01-phase14-final-report.md), [`Phase 15 final report`](benchmarks/2026-09-01-phase15-final-report.md), [`Phase 16 final report`](benchmarks/2026-09-01-phase16-final-report.md)): P0 lemma 40%, P1 lemma 63%, P3 lemma 64%. No-word hallucination is a model capability limit. Phase 15: deterministic answerability projection (`only_answerable()`) enforces the Answerable contract on model output. Phase 16: verified production flow separates raw model quality from product-visible quality. Eliminates 3/6 misleading entries (C01, C02, E03 unanswerable field hallucination). Remaining 3/6 (A04, E02, B03) are model-dependent errors (M6 work). M5 pipeline correctness is closeable. M6 model quality remains open. Counterpart invariant satisfied: no model calls when `word=None`. Zero performance regression. The product label `AI 설명 · 사실 확인 필요` is doing the work it was designed to do |

The trigger and the card are built. `Ctrl+Alt+D`, not a button, because
the overlay passes clicks through
([ADR-0004](decisions/ADR-0004-native-overlay.md)) and a card that could
be clicked would be a card that takes focus. Verified end to end by
`lens --selftest`, which now runs the deep path on the sentence it just
translated.

The deep cache is built: its own map, its own keys, its own prompt
version, per ADR-0005. Measured through `--selftest`, a remembered
explanation comes back in **7 microseconds** against 4.4 seconds.

Copy is built: `Ctrl+Alt+C`, the sentence and its answer, carrying the
AI-explanation label when what is copied is an explanation.

**The deep path now also answers where the quick path correctly refuses
to.** Text already in the reader's language is never translated, and
until 2026-08-06 that meant Lens did nothing with it at all. It is now
remembered and `Ctrl+Alt+D` explains it, with a prompt that tells the
model the reader can read this and wants the part they did not know.
That is the first thing this product does that is not translation, and
it needed no new acquisition, no new card and no new trigger.

It is also offered rather than hidden. `reach` says the key once a
session, on the third hover that produced text the reader can already
read, on a card, because a release binary has no console. A capability
nobody can discover is the silent product that module exists for.

Save is built too, which is M8's first item arriving early because it is
the same keypress-and-a-card shape the two before it are. `Ctrl+Alt+S`
appends the card to `saved.md` beside the settings file, in plain
Markdown, carrying the AI-explanation label when what is saved is an
explanation.

The developer and exam profiles are built, 2026-08-18. A profile is one
line in the deep system prompt and a name in `settings.toml`. It changes
what is asked for rather than what is filtered afterwards, which is the
rule `Answerable` already followed, and it does **not** change the schema:
all five fields stay required, because one list builds the schema and
reads the answer back and the failure when those disagree is silent.

Measured over twelve runs a profile
([`what a profile changes`](benchmarks/2026-08-18-what-a-profile-changes.md)):

| Claim | Result |
|---|---|
| developer drops the part of speech | 12 of 12 |
| developer drops the clause structure | 11 of 12 |
| exam fills the clause structure | 11 of 12, against 7 for the shipped profile |
| developer spends the note on jargon | **1 of 24, and left alone.** The shipped instruction says most sentences need no note, and it is right; a profile line strong enough to beat it would put a note on every card |
| fewer rows are faster | no. The rows a profile drops are short and the sense line dominates |

The evaluation harness is built
([`deep_eval.rs`](../crates/lens-translate/src/deep_eval.rs)): 25 entries,
9 test functions, prompt A/B, model comparison, counterpart analysis. The
failure taxonomy is recorded
([`M5 failure taxonomy`](benchmarks/2026-09-01-m5-failure-taxonomy.md)).
The P1 prompt (lemmatisation examples) was tested on the full corpus and
improved lemma accuracy (+23pp) but caused POS regression (-8pp) and did
not reduce misleading rate (24%). The gate stays open pending a prompt
that improves lemma without regressing POS or increasing misleading rate.

## M6 · macOS acquisition

The AX adapter behind the same `TextAcquirer` trait: point hit test,
character range, string for range, bounds for range, permission flow,
Retina and Spaces coordinate handling.

**Gate** · a macOS compatibility matrix comparable to M1's; `lens-core`
unchanged by the addition, which is the test of whether
[ADR-0003](decisions/ADR-0003-rust-core-platform-adapters.md) held.

**IMPLEMENTATION: complete.** All macOS AX acquisition code is merged and
CI passes on both platforms. `lens-core` is unchanged (ADR-0003 held).
What remains is real-world measurement on actual macOS applications,
which requires Accessibility permission and is tracked as validation debt.

Standing, 2026-09-06:

| Condition | Automated | Real-world |
|---|---|---|
| lens-core unchanged | CLOSED (no platform-specific additions to core) | N/A |
| macOS compatibility matrix comparable to M1 | OPEN | **AWAITING** Accessibility permission |
| Retina coordinate handling | CLOSED (unit tests, scale factor tests) | **AWAITING** real Retina display |
| Spaces coordinate handling | CLOSED (compile-time, CG API) | **AWAITING** real Spaces test |
| Permission flow | CLOSED (unit tests, CI skip on no-permission) | **AWAITING** real permission grant |

## M7 · macOS overlay and Vision OCR

`NSPanel` overlay, accessibility and screen-recording onboarding staged
separately, ScreenCaptureKit crops, Vision recognition.

**Gate** · a user who never uses OCR is never asked for screen
recording; Spaces and full-screen verified; Retina coordinates match.

**IMPLEMENTATION: complete.** NSPanel overlay, ScreenCaptureKit capture
with per-display selection and sourceRect crop, Vision OCR with
per-word geometry via `boundingBoxForRange:`, BGRA pixel format
canonical throughout, overlay exclusion via CGWindowID. CI passes on
both platforms. What remains is real-world measurement, which requires
Screen Recording permission and is tracked as validation debt.

Standing, 2026-09-06:

| Condition | Automated | Real-world |
|---|---|---|
| Non-OCR user never asked for screen recording | CLOSED (AX and OCR are separate paths, OCR only when explicitly triggered) | **AWAITING** real permission flow |
| Spaces verified | CLOSED (compile-time, CG API) | **AWAITING** real Spaces test |
| Full-screen verified | CLOSED (compile-time) | **AWAITING** real full-screen app |
| Retina coordinates match | CLOSED (unit tests, scale factor tests) | **AWAITING** real Retina display |
| Overlay never steals focus | CLOSED (NSPanel config, unit tests) | **AWAITING** real visual check |
| Click-through works | CLOSED (NSPanel config) | **AWAITING** real visual check |
| Overlay exclusion from OCR | CLOSED (CGWindowID in SCContentFilter) | **AWAITING** real compositor test |

## M8 · Learning

Saved words and sentences, notes, review list, export. Saving is always
explicit; nothing is captured automatically.

**Gate** · nothing is stored without a user action; export round-trips;
deletion is real.

**IMPLEMENTATION: complete.** Library (SQLite, 78 tests), Manual Review
(Later/Known/Archive, no scheduling), export (MD + JSON + JSONL),
JSONL round-trip, JSON semantic round-trip, secure wipe, real deletion.
All gate conditions have automated test evidence.

Standing, 2026-09-08:

| Condition | State |
|---|---|
| Nothing stored without user action | CLOSED: Save is explicit. `keeping::append()` has exactly 3 callers: win.rs:1305 (Ctrl+Alt+S), detail_view.rs:175 (Tauri Save button), win.rs:2925 (self-test, temp file). No hover/Quick/Deep/SessionRecent path calls append. `append_creates_both_files_from_nothing` proves dual-write. `session_recent_does_not_survives_drop` proves SessionRecent is memory-only. macOS `execute_save()` uses `save_card()`, writing to both saved.md and library.db. |
| Export round-trips | CLOSED: JSONL: `jsonl_round_trip_preserves_items_and_tags`, `jsonl_semantic_round_trip`, `jsonl_round_trip_preserves_review_state`. JSON: `json_export_semantic_round_trip` (all fields + tags + review_state). Markdown: `exporting_generates_saved_md_format` (export→parse; human-readable format, not machine re-import). |
| Deletion is real | CLOSED: `single_delete_survives_reopen` proves delete(id)→drop→reopen→absent with surviving items and tags. `secure_wipe_survives_reopen` covers bulk. `library_delete_does_not_affect_history` confirms isolation. Archive is a review_state value, not a separate operation. |

## Product Shell · Tauri Control Center

A Tauri webview window implementing the surface [ADR-0012](decisions/ADR-0012-three-product-surfaces.md)
decides: Status, Library, History, and General (settings) sections,
opened from the tray icon. Tray left-click opens the Control Center;
right-click routes to a specific section. The product works with the
Control Center closed: the tray icon, the hover card, and the hotkeys
are independent of it.

**Gate** · the Control Center opens from the tray without stealing
typing focus; it opens on the first run so the reader discovers the
management surface without needing to know about the tray icon; a
second launch brings the existing instance's Control Center to the
foreground; the product works with the Control Center closed; and
`lens-core` is unchanged by the addition.

**IMPLEMENTATION: complete.** All code is merged and verified by
automated tests. Human verification of first-run discovery and
second-launch activation is tracked as validation debt.

Standing, 2026-09-12:

| Condition | State |
|---|---|
| Opens from tray without stealing focus | CLOSED: Tauri webview, tray left-click opens Status section. Automated by Tauri command tests |
| Opens on first run | CLOSED: `first_run` detection in `win.rs` calls `show_window()`. PR #462 merged. **Human verification pending** |
| Second launch brings window forward | CLOSED: named activation event in `single.rs`; event-watcher thread converts HANDLE signal to Tauri UI action; polling loop owns the action. PR #459 merged. Test isolation uses PID + monotonic counter |
| Product works with it closed | CLOSED: hover card, tray, hotkeys, and acquisition are independent of the Control Center. Tauri is an optional surface over the same `HoverSession` |
| lens-core unchanged | CLOSED: no platform-specific additions to core; ADR-0003 held |

## What no milestone here covers

Every gate condition above is a measurement: an acquisition rate, a
latency, a window in a matrix. None is about a person, so this roadmap
can go entirely green while nobody outside the machine that wrote it has
ever seen a card.
[`what-finished-means.md`](what-finished-means.md) states the three
gates that belong at the end and cannot be automated. It adds no
milestone here, on purpose.

## After M8 · Mori integration

A process boundary carrying learning items on an explicit user action.
Never a shared library, never a read of Mori's database
([ADR-0006](decisions/ADR-0006-no-shared-package-yet.md)).

## Not on this roadmap

Automatic full-screen translation, a browser extension, cloud sync,
accounts, teams, flashcard generation, bundled models, fine-tuning, and
live video subtitle translation. Each is excluded on purpose; adding one
is a decision, not a task.
