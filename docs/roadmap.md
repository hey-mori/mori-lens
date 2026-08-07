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
| a full-screen application | **open** |
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
| VS Code ([#14](https://github.com/swarea/mori-lens/issues/14)) | 7 of 247 points, all `50020` labels | the editing surface exposes nothing |
| KakaoTalk ([#34](https://github.com/swarea/mori-lens/issues/34)) | 14 of 104, **1 separable target** | one blob, no way to aim inside it |

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

Standing, 2026-08-06. The model side is built and measured; nothing in
the product calls it.

| Condition | State |
|---|---|
| quick-path latency unchanged | measured: 1,607ms before an analysis and 1,668ms after, sharing one process, one endpoint and one model lease |
| never called without a user action | held by construction: `lens-core::session` has no reference to an `Analyser` and there is no path from a settled hover to one. The only caller is a `Ctrl+Alt+D` press |
| a schema violation degrades | built: a missing field is an empty field, an answer that was not JSON says nothing, and an analysis that says nothing is a typed error rather than a card of headings over blanks |
| output labelled as an AI explanation | built and shown: `DeepAnalysis::LABEL`, first in the footer because a card cuts from the end |
| the explanations are right | **open**, and the reason the label exists. `passed` came back with 원형 `passed` rather than `pass` |

The trigger and the card are built. `Ctrl+Alt+D`, not a button, because
the overlay passes clicks through
([ADR-0004](decisions/ADR-0004-passive-overlay.md)) and a card that could
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

Not started: save, and the developer and exam profiles.

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
