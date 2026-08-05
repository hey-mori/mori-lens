# Windows application compatibility

**Status · started. Five windows measured across two sessions, four of
the eight target applications still unmeasured.**

The full set is the gate on Milestone 2
([ADR-0008](../decisions/ADR-0008-probe-before-product.md)). Nothing here is filled
in from documentation or from expectation. A row exists only after a
probe run produced it.

## How to produce a row

Two instruments, and the sweep is the one to reach for first.

```bash
cargo run -p hover-probe -- sweep --only chrome
```

Samples a grid across the window without moving the pointer, and reports
acquisition rate, on-target rate, latency, and the wake's effect.
**The target window must be in front**: see the occlusion limit below.

```bash
cargo run -p hover-probe -- --auto --expect <word>
```

The follow instrument, for spot-checking that a specific word resolves
to the word you meant. The sweep cannot judge that, because it does not
know which pixels carry prose.

**Check the build sha the probe prints against
`git rev-parse --short HEAD` before trusting a run.** `cargo test` does
not rebuild the binary target, so a check-then-run sequence can measure
code that is minutes out of date and say nothing about it. That cost a
full run on 2026-08-04.

## The occlusion limit

A sweep can only measure a window that is **actually visible**.
`ElementFromPoint` answers with whatever is topmost at a coordinate, so
sampling a covered window measures the window on top of it. The probe
now checks the owner of every point and skips the covered ones, which
means a background window reports `0/0 (N occluded)` rather than
someone else's numbers.

The consequence for a compatibility session: bring one application
forward, sweep it, then the next. The probe deliberately does **not**
activate windows itself. A measurement tool that rearranged the desktop
it was measuring would change the thing it is trying to observe.

Record the environment with the run, because these results are not
portable across machines. The shape, filled in from the actual machine
rather than an example:

```
date          2026-08-04
build         <the sha the probe prints, checked against HEAD>
os            Windows 11 Home 26200
monitors      2 side by side
  primary     1920x1200 physical at 125%, origin (0, 0)
  secondary   1920x1080 physical at 100%, origin (1920, 0)
```

## Columns

| Column | Meaning |
|--------|---------|
| Application | name and version |
| Framework | Win32, WPF, WinForms, WinUI, Chromium, Electron, Java, Qt |
| Element | did `ElementFromPoint` return a usable element |
| TextPattern | does the provider expose `UIA_TextPatternId` |
| Word | did the word range resolve, and was it the right word |
| Context | did the enclosing line or paragraph read |
| Bounds | do the rectangles match what is on screen |
| P50 / P95 | element-to-text latency, milliseconds |
| Wake | did the Chromium accessibility wake change the outcome |
| Failure | the typed reason when it failed |

Judgement calls, so they are made the same way twice:

- **Word · correct** means the resolved word is the one under the
  pointer, at the pointer's centre. If pointing at the middle of a word
  yields its neighbour, that is `wrong`, not `ok`.
- **Bounds · ok** means the rectangle visibly encloses the text. Check
  it, do not infer it from plausible numbers.
- A blank cell means not measured. It never means "worked".

## Target set for the M1 gate

These eight are the gate. Each covers a distinct provider family, and
between them they cover where a user actually meets a second language.

| Application | Framework | Why it is in the set |
|-------------|-----------|---------------------|
| Notepad | Document provider, measured | the simplest text case; measured, and it is **not** the Win32 `EDIT` family |
| Chrome | Chromium | where most reading happens; needs the accessibility wake |
| Edge | Chromium | same engine, different shell and PDF viewer |
| VS Code | Electron | code and documentation, a heavy renderer; measured, and the editor exposes nothing |
| Word | Win32 / Office | the document case |
| Edge PDF viewer | Chromium PDF | text PDFs, the boundary before OCR |
| Windows Settings | WinUI | modern XAML controls |
| Slack or Teams | Electron | messenger under load |

## Results

<!--
Do not pre-create empty rows: an empty row reads as a result. Only add a row a
run actually produced, and say which run.
-->

### Sweep, 2026-08-04

```
build         d1e236b-dirty
os            Windows 11 Home 26200
monitors      2 side by side
  primary     1920x1200 physical at 125%, origin (0, 0)
  secondary   1920x1080 physical at 100%, origin (1920, 0)
command       hover-probe sweep --points 120 --budget 10
```

Full numbers and the latency analysis:
[`../benchmarks/2026-08-04-acquisition-sweep.md`](../benchmarks/2026-08-04-acquisition-sweep.md).

| Window | Framework | Element | TextPattern | Controls | On-target | Distance p50 / max | Latency p50 / p95 | Wake |
|---|---|---|---|---|---:|---:|---:|---|
| Chrome, 956x925, @100% | Chromium | yes | yes | `50020` | **86 to 100%** | 0px / 3px | 7.9 / 12.4ms | +2 acquisitions |
| Chrome, 1777x1072, @100% | Chromium | yes | yes | `50030`, `50004` | **5%** | 322px / 935px | 10.4 / 19.3ms | already warm |
| Claude desktop, 1884x1112, @125% | Electron | yes | yes | `50020` | **69 to 75%** | 0px / 14px | 9.3 / 20.2ms | already warm |

### Sweep, 2026-08-04, second session

Same machine and monitors. Build `480603f`, confirmed against
`git rev-parse --short HEAD`. Each application brought to the front by
hand before its run.

| Window | Framework | Acquired | On-target | Distance p50 / max | Controls | Latency p50 / p95 |
|---|---|---:|---:|---:|---|---:|
| VS Code, 1200x800, @100% | Electron | 7/247 | **86%** | 0px / 7px | `50020` | 6.5 / 13.1ms |
| Notepad, 761x583, @100% | Win32 era | 45/70 | **11%** | 588px / 696px | `50030` | 12.4 / 44.5ms |

Notepad was 182 of 252 points occluded, so its row rests on the visible
70. The probe verified ownership per point, so those 70 are genuinely
Notepad's, but a fully visible run would be a stronger sample.

#### VS Code exposes almost nothing, and it matters most

**7 acquisitions out of 247 sampled points, every one of them a `50020`
Text control.** Text is the control type of a label. No `50030` Document
and no `50004` Edit appeared anywhere in the window.

The strong reading, which the numbers indicate rather than prove: the 7
are window chrome, tab titles and the status bar, and **the editor
itself returned nothing**. Monaco builds an accessible representation of
its text only when it believes an assistive client is present;
`editor.accessibilitySupport` defaults to `auto`, which detects a screen
reader and Lens is not one.

That is a product problem rather than a curiosity. A developer reading
English documentation and error messages is the first user in
[`../mori-lens-project-goal.md`](../mori-lens-project-goal.md) §3, and a
code editor is where they do it. Tracked as issue #14.

**Confirmed by a second run**, build `dfdebcb`, same window:

```
acquired 2/247 (1%)   on-target 2/2 (100%)   control 50020x2
unsupported 245
```

Fewer acquisitions than the first run, not more, and again **not one**
`50030` or `50004` anywhere. Two independent sweeps of the same window
agree that nothing in it reports a text-editing surface. The handful of
points that do resolve are labels, and they resolve perfectly, which is
consistent with chrome rather than content.

Still not established: exactly what was displayed during either run.
Both were driven remotely. That ambiguity no longer changes the
conclusion, because the two runs bracket different window contents and
neither produced an editing control. It does still matter for the next
step, which is whether `editor.accessibilitySupport` changes anything.

#### Notepad is a Document provider, not the `EDIT` case

`50030` Document on every one of its 45 acquisitions, and 64% of visible
points acquired. Windows 11's Notepad is **not** the Win32 `EDIT` family
that [ADR-0002](../decisions/ADR-0002-accessibility-first-acquisition.md)
leans on; the genuine `EDIT` case is the harness control below, which
returns `unsupported`.

Its 11% on-target rate is the empty-space pattern again, in its purest
form yet: **40 misses, all 40 with the point above the resolved
rectangle**, over 8 distinct rectangles. Nothing scattered, nothing
symmetric. The gate correctly accepts 0 of 40 at 588px.

A new failure reason appeared here for the first time: `empty 15`,
alongside `unsupported 10`. Worth watching, because `empty` is a
provider answering with no text where `unsupported` is a provider with
no text interface at all.

**Still unmeasured**, because each was fully occluded during a run and
the probe correctly refused to report someone else's numbers: Edge,
Word, the Edge PDF viewer, Windows Settings, and an Electron messenger.
Bringing each forward and sweeping it is the remaining work on issue #3.

### Can it tell which message the pointer is on?

Measured 2026-08-05 on an Electron chat window, because the on-target rate
cannot answer this on its own: **one huge correct target scores 100%**. So the
sweep now also counts how many *distinct* rectangles the on-target hits resolve
to.

```
claude (Electron), 1884x1112, 240 points
  acquired    25/240 (10%)
  on-target   22/25 (88%)   distance p50 0px · p95 9px · max 22px
  separable   22 distinct targets among 22 on-target hits (1.0 each)
  control     50020 Text x25 · word unit: word x25
```

**Every on-target hit resolved to a different rectangle.** The window is not
one blob with a pointer somewhere in it: each message, and within it each word,
is its own element. Combined with a 22px worst-case error, the pointer lands on
a specific word and the right one.

That is the good case, and it is the case a messenger presents. The bad case is
the one above: a document or edit control is a single scrollable region where
`RangeFromPoint` answers with the nearest text anywhere in it, and the answer
can be 1,465px away.

So the honest summary of "does it know what you pointed at":

| Surface | Which window | Which sentence |
|---|---|---|
| labels, chat, most UI | reliable | 86 to 100%, each hit separable |
| document, editor body | reliable | 3 to 18%; the rest is suppressed rather than shown wrong |
| VS Code's editor | reliable | nothing is exposed at all (#14) |

**Which window** is not in doubt anywhere: `ElementFromPoint` performs the same
hit test that decides where a click goes, and the probe verifies the owning
window per point, which is how the occlusion bug was caught.

### The finding that changes the design

On-target rate splits by control type, and the split held across all
three runs and every application seen:

| Control type | On-target | Distance p50 |
|---|---:|---:|
| `50020` Text, a label | 86 to 100% | 0px |
| `50030` Document, `50004` Edit | 3 to 18% | 82 to 322px |

`RangeFromPoint` on a label returns the label. On a document or edit
control it returns the nearest text anywhere in that scrollable region,
which for a point below the last line is hundreds of pixels away, and it
**reports success**. There is no error to catch.

So inside a document the distance check is the only thing standing
between the product and confidently explaining a sentence the user is
not looking at. That is issue #5, answered: the threshold is required,
not a refinement.

### Win32 `EDIT`, measured against a control we built

The matrix had no genuine Win32 application, and the one row that seemed
to speak to it turned out to be another window's numbers (below). So the
control is now built rather than waited for:
`crates/platform-windows/src/live_tests.rs` creates a real window with
two `EDIT` controls and reads each.

| Control | Result |
|---|---|
| plain `EDIT` | `unsupported`, reproducibly |
| `EDIT` with `ES_PASSWORD` | `protected`, declared by the provider |

**Mori's finding is confirmed.** A plain Win32 `EDIT` exposes no text
pattern at all. That is not a defect to fix, it is the reason the OCR
fallback exists rather than being a defensive nicety
([ADR-0002](../decisions/ADR-0002-accessibility-first-acquisition.md)),
and it now rests on this project's own measurement rather than on a
finding carried across from another repository.

Note what this does **not** say: modern applications rarely use a raw
`EDIT`. Windows 11's Notepad, WinUI apps, and every browser use
something else. This is the floor for a specific legacy control family,
not a prediction about any application a user runs.

### A retracted claim, kept as a worked example

The two runs before the occlusion check reported Notepad at 80%
acquired with control types `50004` and `50030`, which looked like a
clean correction to Mori's finding that Win32 `EDIT` exposes no text
pattern.

**It was not Notepad.** Notepad sat at (2974, 288) 761x583, entirely
inside a Chrome window at (2043, 0) 1777x1072. The occluding Chrome
window reports exactly the profile those rows carried, `50030` plus
`50004`. The numbers were Chrome's, filed under Notepad's name.

Notepad is unmeasured, and Mori's `EDIT` finding is neither confirmed
nor corrected here. Testing it also needs a genuine WinForms or
old-Win32 application, which Windows 11's Notepad may or may not still
be; that is itself unverified.

This is left in the document rather than deleted because it is the
cleanest illustration of why the occlusion check exists: the wrong
numbers were entirely plausible, internally consistent, and would have
been believed.

### Smoke check, 2026-08-04

Not a result and not a row. Recorded because it is the evidence that
the chain runs at all on a live desktop, which is a different claim
from "the chain works well enough".

```
build     initial workspace, debug
command   hover-probe --auto --dwell 200
duration  10s, pointer left wherever it was

SourceTree  ok  110ms  word=0 sent=1 para=1 rects=0/4 dist=7 ctrl=50030
```

What this establishes: COM initialises, `IUIAutomation` is created,
`ElementFromPoint` resolves an element, the element exposes a text
pattern, `RangeFromPoint` returns a range, and bounding rectangles come
back (four of them for the context range).

What it does not establish, and must not be read as: that the resolved
text was the right text. `word=0 sent=1` is a one-character context
with no word in it, which is what a pointer parked on chrome rather
than on prose produces. The 110ms is a first call including COM
warm-up and is not a latency measurement.

The gate still needs deliberate hovering over real prose in the eight
target applications.

## Known limitations, before measurement

Stated so a run does not rediscover them as bugs:

- An application running elevated cannot be read by a Lens running
  without elevation (UIPI). Lens does not run elevated
  ([`../privacy-model.md`](../privacy-model.md) §4).
- Games, video, images, and scanned PDFs expose no text and are expected
  to fail with `Unsupported`. That is the OCR fallback's territory (M5),
  not a defect.
- A protected field returns `Protected` and is correct behaviour.
