# Windows application compatibility

**Status · empty. No application has been measured yet.**

That sentence is the gate on Milestone 2
([ADR-0008](../decisions/ADR-0008-probe-before-product.md)). Nothing here is filled
in from documentation or from expectation. A row exists only after a
probe run produced it.

## How to produce a row

```bash
cargo run -p hover-probe -- --auto --expect <word>
```

Then hover over ordinary body text in the target application for a few
seconds, over a heading, over a single word, and near the edge of a
line. Press Ctrl+C. The probe prints a summary; transcribe it here.

Record the environment with the run, because these results are not
portable across machines:

```
date          2026-08-04
build         <git sha>
os            Windows 11 26200
scaling       150% primary, 100% secondary
monitors      2, secondary left of primary (negative X)
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
| Notepad | Win32 / RichEdit | the simplest possible case, and Mori found Win32 `EDIT` exposes no text pattern at all |
| Chrome | Chromium | where most reading happens; needs the accessibility wake |
| Edge | Chromium | same engine, different shell and PDF viewer |
| VS Code | Electron | code and documentation, a heavy renderer |
| Word | Win32 / Office | the document case |
| Edge PDF viewer | Chromium PDF | text PDFs, the boundary before OCR |
| Windows Settings | WinUI | modern XAML controls |
| Slack or Teams | Electron | messenger under load |

## Results

<!--
One table per application. Do not pre-create empty rows: an empty row
reads as a result, and there are none yet.
-->

_No gate runs recorded._

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
