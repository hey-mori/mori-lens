# Mori Lens · project goal and honest inventory

This file is the only place allowed to claim that a capability exists.
If a feature is not listed in §4 as implemented, it is not implemented,
whatever a design document says about it.

## 1. What Mori Lens is

A local-first language layer for the whole screen. The user rests the
pointer on a word or a sentence in any application; Lens reads the real
text from the operating system's accessibility tree, works out the word,
the sentence, and the paragraph around the pointer, detects the source
language, and shows the meaning in the user's target language in a small
native card beside the original.

The interaction is the product. No selecting, no copying, no hotkey, no
region capture. The user does not change what they are doing in order to
understand a sentence.

## 2. What it is not

- **Not a screen translator.** It does not capture the screen and OCR
  it. OCR is a fallback for surfaces that expose no text, and in the
  first version it is manual only ([ADR-0002](decisions/ADR-0002-accessibility-first-acquisition.md)).
- **Not a browser extension.** It works in editors, PDF viewers,
  messengers, and settings screens, not only in a browser.
- **Not a chat interface.** There is no prompt box. The pointer is the
  query.
- **Not a screen recorder.** Nothing is captured on a timer, and no
  captured image is ever written to disk.
- **Not a Mori feature.** It is a separate product with a separate
  binary that runs with Mori not installed
  ([ADR-0001](decisions/ADR-0001-independent-repository.md)).

## 3. Who it is for

Someone who reads a second language every day and loses time to it:
developers reading English documentation and error messages, learners
reading real material rather than textbook sentences, professionals in
documents and messengers. The common thread is that they are *already
reading something*, and stopping to translate breaks the reading.

## 4. Implemented versus planned

Status vocabulary: **shipped** (works and was measured), **built,
unverified** (code exists, no live measurement), **planned** (designed,
not written), **out of v1** (deliberately excluded).

| Capability | Status |
|------------|--------|
| Hover intent state machine (dwell, radius, cooldown, stale discard) | built, unverified · covered by unit tests, never driven through a real reading session |
| Windows UI Automation text acquisition under the pointer | **shipped** · measured on 3 windows, 2026-08-04 |
| Word / sentence / paragraph segmentation | built, unverified · unit tested, no human has confirmed a resolved word is the intended one |
| Secure-field refusal | **shipped** · fires as `Protected(Declared)` against a real `ES_PASSWORD` control, and discriminates from a plain one |
| Hover probe, follow mode | built, unverified |
| Hover probe, sweep mode | **shipped** · produced the 2026-08-04 measurements |
| Per-application compatibility matrix | in progress · 3 of 8 target applications |
| Native overlay (Windows) | planned · M2 |
| Quick translation via Ollama | planned · M3 |
| Deep analysis (grammar, structure) | planned · M4 |
| Manual OCR fallback (Windows) | planned · M5 |
| macOS acquisition | planned · M6 |
| macOS overlay and Vision OCR | planned · M7 |
| Saved vocabulary and sentences | planned · M8 |
| Mori integration | planned · after M8, as a process boundary |
| Automatic full-screen OCR | out of v1 |
| Browser extension | out of v1 |
| Cloud sync, accounts, teams | out of v1 |
| Bundled models | out of v1 ([ADR-0007](decisions/ADR-0007-local-first-inference.md)) |

A capability becomes **shipped** when a run against named applications is
recorded in
[`compatibility/windows-app-matrix.md`](compatibility/windows-app-matrix.md).
That gate is [ADR-0008](decisions/ADR-0008-probe-before-product.md).

Two capabilities passed it on 2026-08-04. The rest have unit tests,
which prove the logic and prove nothing about a desktop.

**What the measurement found, since it changes what "works" means
here:** acquisition succeeds, at p50 10ms, and returns the *right* text
86 to 100% of the time on label controls. Inside a document or an edit
control it returns the nearest text in the whole scrollable region, up
to 1,465px from the pointer, and reports success. Acquisition is
therefore solved and *targeting* is not
([`benchmarks/2026-08-04-acquisition-sweep.md`](benchmarks/2026-08-04-acquisition-sweep.md)).
No overlay should be built until that has an answer, because a card
placed on the wrong sentence is worse than no card.

## 5. The three properties that define the product

Stated here because they are what a change should be judged against.

**Semantic hover.** Lens reads the actual word and sentence under the
pointer, not the pixels around it. This is what makes it accurate about
boundaries, cheap at idle, and able to explain a word *in its sentence*.

**Two-speed AI.** Translation is immediate. Grammar, structure, and
vocabulary explanation happen only when asked. A hover never costs a
four-second generation ([ADR-0005](decisions/ADR-0005-two-speed-ai.md)).

**Local-first privacy.** What the user is reading does not leave the
machine by default, nothing is recorded, and the engine in use is
visible on the card ([`privacy-model.md`](privacy-model.md)).

## 6. How success is measured

Not by feature count. By these, per application:

- accessibility acquisition success rate
- correct-word rate (the resolved word is the one under the pointer)
- time from pointer settling to a card being drawable
- unwanted-card rate (a card the user did not want)
- missed rate (text the user wanted explained and got nothing)

The first two are what Milestone 1 exists to measure. The rest need an
overlay and are measured from Milestone 2.
