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
| Hover intent state machine (dwell, radius, cooldown, stale discard) | built, unverified · unit tested, and the card lifetime it feeds now has its own tests |
| Windows UI Automation text acquisition under the pointer | **shipped** · measured across all eight target applications, and a wedged one is abandoned after 800ms rather than after two minutes |
| Word / sentence / paragraph segmentation | built, unverified · unit tested, no human has confirmed a resolved word is the intended one |
| Secure-field refusal | **shipped** · fires as `Protected(Declared)` against a real `ES_PASSWORD` control, and discriminates from a plain one |
| Hover probe, follow mode | built, unverified |
| Card themes | **shipped** · four presets, contrast checked against the composite over a white page and a black one |
| Card rendered with per-pixel alpha | **shipped** · Direct2D and DirectWrite: the text is opaque whatever the surface's opacity, and the corners are round and antialiased. Neither blurred nor shadowed: the shadow was rebuilt three times and read as a box every time, so it was removed rather than tuned a fourth. See [ADR-0011](decisions/ADR-0011-themed-card-glass-later.md) |
| Pause control | **shipped** · Ctrl+Alt+L or the tray icon, and paused means the pointer is not sampled |
| Settings file | **shipped** · `%APPDATA%\Mori Lens\settings.conf`, written with every option on first run, and a line it does not understand is reported at startup rather than ignored |
| Per-application tiers | **shipped** · set in the settings file; password managers are denied before the file is read and cannot be re-enabled by omission |
| No console window | **shipped** · a Windows subsystem binary that takes the launching terminal's console when there is one, guarded by a test that reads the PE header |
| Waiting state | **shipped** · the card says it heard you after 180ms instead of leaving the screen empty for the second a model takes, and never for a cached sentence |
| Start with Windows | **shipped** · off unless turned on from the tray menu, per-user run key, no elevation |
| Tray icon | **shipped** · shows whether Lens is listening, pauses on a click, quits from its menu, and survives Explorer restarting |
| Translation cache | **shipped** · in memory, keyed on model and prompt version |
| Hover probe, sweep mode | **shipped** · produced the 2026-08-04 measurements |
| Per-application compatibility matrix | **shipped** · all 8 target applications, 2026-08-05 |
| Native overlay (Windows) | **shipped** · never takes focus, paints without a blank or hidden frame while streaming, clicks pass through, and lands on the monitor its text is on |
| Tells the reader an application cannot be read | **shipped** · after three settled hovers fail as unsupported in one application, a card names the OCR hotkey, once per application per session. Never for a protected field, and never a key that is unregistered or has no recogniser behind it |
| Failure messages that say what to do | **shipped** · an absent Ollama names ollama.com, a missing model names the `ollama pull` command, on the card and in the tray tooltip |
| Quick translation via Ollama | **shipped** · warm median around 1.6s, streamed, cached in memory, model resolved at startup |
| Deep analysis (grammar, structure) | planned · M5 |
| Manual OCR fallback (Windows) | **shipped** · Ctrl+Alt+O reads a band around the pointer, hides the card first, resolves the word under it, and feeds the same translation and card path. Never automatic, never to disk. Measured against VS Code on 2026-08-06, the application that prompted the milestone: 336 words recovered where accessibility resolves 9 points in 260 |
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

Seventeen capabilities have passed it. The rest have unit tests, which
prove the logic and prove nothing about a desktop.

What is still missing before this is something a person installs: a
signed binary and an installer, so Windows does not warn on first run.
Building it yourself is the current answer and it is not one most people
will accept.

**What the measurement found, since it changes what "works" means
here:** acquisition succeeds, at p50 10ms, and returns the *right* text
86 to 100% of the time on label controls. Inside a document or an edit
control it returns the nearest text in the whole scrollable region, up
to 1,465px from the pointer, and reports success. Acquisition is
therefore solved and *targeting* is not
([`benchmarks/2026-08-04-acquisition-sweep.md`](benchmarks/2026-08-04-acquisition-sweep.md)).
No overlay should be built until that has an answer, because a card
placed on the wrong sentence is worse than no card.

**The wiring now has tests too.** The decisions in
`apps/lens/src/main.rs` about when a card appears, stays, and goes are
extracted into `lifecycle.rs`, which decides without acting so a whole
reading session can be played through it in microseconds. The one bug a
user has reported in this product lived there and survived a green
suite; it is now a test.

What is still untested in that loop is the part that is genuinely I/O:
sampling the pointer, sending a job, and receiving an answer. Those are
thin, and they are exercised by `--selftest` and the live checks rather
than by anything automatic.

**The second measurement session found a harder problem than targeting.**
VS Code returned 7 acquisitions across 247 sampled points, all of them
label controls, which indicates its editor exposes no text to read at
all. A developer reading documentation and error messages is the first
user named in §3, and an editor is where they do it. Tracked as issue
#14; it is a question about reach rather than about accuracy, and no
amount of getting targeting right answers it.

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
