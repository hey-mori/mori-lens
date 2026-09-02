# Windows application compatibility

**Status · all eight gate applications measured, 2026-08-05.** Windows
Settings and the Edge PDF viewer were the last two, and closing them
corrected an earlier claim: both windows the 2026-08-04 run recorded as
"Edge" were the PDF viewer, so Edge's HTML rendering had never actually
been measured. It has been now, separately.

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

**Half of that changed on 2026-08-07.** `hover-probe ocr --only <name>`
now finds the named window wherever it is, and asks it to draw itself
with `PrintWindow` when it is behind something. VS Code, 74% covered,
rendered at 99.5% ink and gave 97 lines and 2,232 characters. The row
says so rather than leaving a reader to infer it:

```
rendered  the window is behind another; it drew itself, 99.5% ink.
```

The sweep is unchanged and cannot change. `ElementFromPoint` answers
with the topmost window whatever the pixels say, so the accessibility
half of a covered run is `0 over this window` and reports nothing. What
this removes is the need to arrange a desktop to measure **how much text
a window has**, which is the question OCR answers.

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
| Edge | Chromium | measured on HTML, separately from its PDF viewer |
| VS Code | Electron | code and documentation, a heavy renderer; the editor exposes nothing to accessibility and is readable by OCR, measured 2026-08-06 |
| Word | Win32 / Office | the document case |
| Edge PDF viewer | Chromium PDF | measured: a text PDF reads like a page |
| Windows Settings | WinUI | measured: sparse, and correct wherever there is text |
| Slack or Teams | Electron | messenger under load |
| KakaoTalk | Win32 / Document provider | measured: one blob, and the messenger a Korean reader actually uses |

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
| VS Code's editor | reliable | **the text is exposed and a pointer does not reach it**, measured 2026-08-07 with screen-reader mode on: six words of one doc comment all found in the tree by handle. The point-based rows below stand and the diagnosis under them does not (#14) |

**Which window** is not in doubt anywhere: `ElementFromPoint` performs the same
hit test that decides where a click goes, and the probe verifies the owning
window per point, which is how the occlusion bug was caught.

### Inventory, 2026-08-05: every window at once, nothing raised

Taken with `hover-probe inventory`, which asks each window's provider by handle
rather than by coordinate, so a covered window answers. The whole desktop in
about three seconds, against minutes of grid sweeping that needed each
application brought to the front.

```
application          elements  withtext    chars  smallest  on-target
WINWORD                   248       191    12658      19px        7/7
msedge                    143       142     5492      13px      11/12   1 not rendered
chrome                    138       133     4761      16px       5/12   2 not rendered, 5 mislocated 46-270px
claude                    680       199     3762      18px      12/12
msedge                     31        30     2178      20px      12/12
chrome                     86        86     1444      20px      12/12
SourceTree                  4         2      800     206px          -   no rendered text to probe
WindowsTerminal             2         2      425      16px        2/2
chrome                      5         5      397      24px        5/5
KakaoTalk                   2         2       40      20px        1/1
EXCEL                       9         8       24      19px        8/8
explorer                    5         3       16      15px        3/3
```

**Word is the best result this project has measured.** 248 text elements,
12,658 characters, and every probe resolving onto its own text. The document
case, which the grid sweep made look like the hard one, is not hard at all when
the question is asked properly.

**Edge works**, on both windows, and was measured without anyone touching it.
So were Word, Excel and Explorer.

**One Chrome window out of four is the genuine outlier**: five probes
mislocated by 46 to 270px. Page-dependent rather than a Chromium property,
since the other three Chrome windows score 5/5, 12/12 and 11/12.

**SourceTree has no rendered text to probe.** Its two text elements report
content but no rectangles, which is a window scrolled or collapsed rather than
a resolution failure.

**KakaoTalk resolves correctly and has almost nothing to resolve**: 40
characters in two elements. That is the finding, and it is not the one the grid
sweep suggested.

#### What this measurement is, and is not

It asks each element's own text pattern. It does **not** exercise the product's
entry point, which is `ElementFromPoint` at a pointer position, and a covered
window still fails there. What it settles is the prior question: whether there
is text at all, whether it is in pieces a pointer could aim at, and whether the
provider locates it correctly when asked about a point that is provably on a
glyph.

Three corrections were needed before these numbers meant anything, and each was
a real error rather than a tuning choice:

1. Probing the **centre of an element's bounding box**, which is padding or an
   empty scroll area often enough to score one window 4 of 12 where the
   corrected probe scores 12 of 12. The point now comes from the text's own
   rendered rectangles.
2. Judging by **strict containment**, the mistake
   [ADR-0009](../decisions/ADR-0009-pointer-distance-gate.md) already corrected
   for the product. The instrument now gates on a line height, as the product
   does.
3. Counting **text that resolved but is not rendered** as a miss. It is not:
   unrendered is scrolled out of view, not mislocated, and the product treats
   it the same way.

### The last two gate rows, 2026-08-05

Build `dea1e18`, per-monitor aware v2, 125% scaling on the primary display.

Neither application had to be raised for the inventory, and neither had
to be arranged by hand for the sweep beyond not being covered.

#### Windows Settings · WinUI

```
application          elements  withtext    chars  smallest  on-target
ApplicationFrameHo…        33        32      327      20px      12/12

sweep  1500x1032 at (114, 70)
  cold   acquired 18/176 (10%)   latency p50 4.9ms · p95 8.3ms
         separable 14 distinct targets among 14 on-target hits
         unsupported 158
  woken  identical, so this provider was never cold
```

**Supported, and sparse.** 10% acquisition is the correct answer for
this window rather than a poor one: a Settings page is mostly empty
space, and `unsupported` is what an empty region should return. The
number that matters is that all 14 acquisitions resolved onto their own
text, and that 14 of them were distinct. A pointer can aim inside this
window.

327 characters is the whole window. Settings is labels and toggles, not
prose, so the reader this product is for meets little here.

#### Edge PDF viewer · Chromium PDF

Measured on a text PDF (a business document, not named here).

```
application          elements  withtext    chars  smallest  on-target
msedge                    143       142     5492      13px      11/12

sweep  992x959 at (2527, 134)
  cold   acquired 116/132 (88%)   latency p50 7.0ms · p95 9.6ms
         separable 8 distinct targets among 8 on-target hits
         unsupported 16
  woken  114/132 (86%), 6 separable
         wake: -2 acquisitions, which means the window changed under the sweep
```

**A text PDF reads like a page.** 88% acquired at 7ms, every hit on
target, 13px as the smallest element. This is the boundary the roadmap
put before OCR, and for a text PDF the accessibility path is simply
enough. A scanned PDF is still OCR's territory and is not this row.

The woken pass losing two acquisitions is the window scrolling or
re-laying-out mid-run, not the wake doing harm.

#### Edge on HTML · Chromium

The row that turned out to be missing. Measured on a local page written
for the purpose, in a new window, so nothing of the user's was disturbed.

```
application          elements  withtext    chars  smallest  on-target
msedge                      8         6     1009      20px        5/5

sweep  992x959 at (2537, 144)
  cold   acquired 108/120 (90%)   latency p50 7.2ms · p95 9.5ms
         separable 8 distinct targets among 8 on-target hits
```

**Supported.** 90% at 7ms, all on target.

#### The correction this run forced

The 2026-08-05 inventory recorded two `msedge` rows and the text above
them said *"Edge works, on both windows"*. Printing window titles showed
both of those windows were displaying **PDFs**. So that sentence was a
claim about the PDF viewer twice, and Edge's HTML rendering, which is
the row the gate actually asks for, had never been measured at all.

Two windows of one process are two results, and the inventory could not
tell them apart. It can now, behind `--titles`, off by default because a
window title is a document name.

#### And one about occlusion, again

The first attempt at the PDF sweep returned `0/12 acquired, 120
occluded`. The new Edge window opened to measure HTML was sitting on top
of it. The sweep reported the occlusion honestly and the number was
still nearly published as a PDF result.

### KakaoTalk, 2026-08-05: the case the product is for, and it is the bad one

```
KakaoTalk, 475x800, 104 points, build 007c8ec
  acquired    14/104 (13%)
  on-target   2/14 (14%)     distance p50 57px · p95 221px · max 221px
  separable   1 distinct target among 2 on-target hits
  off-target  12 points -> 3 distinct rects (4.0 each)   below 7 · above 0 · left 1 · right 4
  gate        would accept 1 of 12 as slack
  control     50030 Document x14
  word unit   unknown 8 · word 6
  unsupported 90
```

**A messenger that behaves like a document, not like the other messenger
measured.** The Electron chat window gave 22 distinct targets from 22
on-target hits; KakaoTalk gives **one**. Twelve off-target hits collapse to
three rectangles. The whole conversation is a single scrollable region with a
pointer somewhere inside it.

The consequence, and it follows from the design rather than being a separate
fault: the distance gate suppresses eleven of the twelve, so **Lens shows
nothing** in KakaoTalk rather than showing the wrong message. Correct
behaviour, and a product that does very little there.

New in this row: `word unit: unknown 8`. Eight of fourteen word expansions came
back with nothing classifiable, which no other application has produced. Worth
understanding before concluding anything about why the on-target rate is so
low, because an empty expansion and a mislocated one are different faults.

#### With a conversation open, it is worse

The friend list above may not have been the surface that mattered, so it was
measured again with a conversation window in front. Twice, once with the
accessibility wake and once without:

| Window | Points | Acquired | On-target | Distance p50 / max | Word unit |
|---|---:|---:|---:|---:|---|
| 475x800 friend list | 104 | 14 (13%) | 14% | 57px / 221px | unknown 8, word 6 |
| 475x1011 conversation | 128 | 7 (5%) | **0%** | 167px / 362px | unknown 6, word 1 |
| 475x1011, wake sent | 98 | 6 (6%) | **0%** | 216px / 370px | unknown 5, word 1 |

**Nothing resolves to where the pointer is.** Not one on-target hit across two
conversation sweeps. The wake changes nothing, which is expected since it is a
Chromium mechanism and this is not Chromium; its woken pass is also
contaminated, 76 of 98 points occluded because a window moved under it.

Two other KakaoTalk windows returned `unsupported` at every single one of 80
and 72 points: no text pattern anywhere.

The consistent signature across all three: control `50030` Document, five to
thirteen percent of points exposing anything at all, word expansion returning
nothing classifiable five times out of six, and every off-target hit resolving
*upward* to one of two or three rectangles.

#### What follows

**KakaoTalk is an OCR case, not an accessibility case.**
[ADR-0002](../decisions/ADR-0002-accessibility-first-acquisition.md) reserves
OCR for surfaces that expose no usable text, and this is one, measured three
times.

That is worth more than a matrix row, because it is the messenger a Korean
reader is most likely to meet a second language in, and the manual OCR fallback
is currently M5, behind deep analysis and after two other milestones. Whether
that ordering still makes sense is a product question, raised in #34 rather
than decided here.

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

### A fourth VS Code run, 2026-08-06, that settles nothing

Build `261ec78`, `hover-probe inventory` and `hover-probe sweep --only
Code`, on a desktop nobody rearranged for it.

Recorded because it looks at first like a contradiction of everything
below, and is not, and the next person to run these two commands will
see the same thing.

The inventory reports VS Code holding **107 elements, all 107 with text,
8,131 characters**, the largest of any window on the desktop, with 12 of
12 sampled elements resolving back into themselves.

The sweep over the same window:

```
Code  1440x900 at (2190, 120)
  cold   acquired 15/67 (22%)   on-target 15/15 (100%)   (193 occluded)
         separable 15 distinct targets among 15 on-target hits
         control 50020x15   unsupported 52
```

22% against the 2.8% in the table above, and every hit on target and
separable.

**It settles nothing, and the reason is the occlusion count.** 193 of
260 points were covered by another window and skipped, so this measured
the visible **26%** of a mostly-buried window, which is its edges: the
tab strip, the activity bar, the status bar. That is exactly where the
`50020` labels live and exactly not where the editor is. A high hit rate
over the chrome is what the rows below already say.

The inventory number has the same shape. It asks each window by handle
rather than by coordinate, so it reaches text the pointer cannot, and
nobody has attributed those 8,131 characters to the editor rather than
to the file tree, the tabs and the command palette. Counting them is not
the same as showing a pointer can reach one.

**What would settle it** is the run the section below did: VS Code in
front, unoccluded, swept and then read with OCR over the same grid. That
run put the editor at zero three times. Until somebody repeats it with
`editor.accessibilitySupport` turned on (#14), the answer stands.

### OCR against VS Code, 2026-08-06: the M4 question, answered

Build `563c9a2`, `hover-probe ocr --only Code --wait 6`, one window at
100% scaling, 1440x900 client area, 260 grid points. Same grid rule as a
sweep, so the two halves are over the same sample.

```
recognition  93 lines · 336 words · 1922 characters in 1652.1ms
median word  28x10px
grid         260 points, 260 over this window, 44 of those on a word

accessibility, over the same points     cold    woken
  acquired anything                       9       9
  the point was inside what it read       9       9

where OCR found a word (44 points)       cold    woken
  acquired anything                       8       8
  the point was inside what it read       8       8   18.2% woken

latency      p50 5.1ms · p95 8.8ms   (accessibility)
```

**OCR reads this window and accessibility does not.** On the 44 points
where there is text to read, accessibility could serve 8. The other 36
are text a reader can see and Lens could not reach at all before this.

Three things worth separating out.

**The 9 accessibility hits are the same 9 the sweeps found.** Eight of
them fall on points OCR also sees, so they are not a different set of
text; they are the chrome and the labels, measured a third way. This is
the third instrument to put VS Code's editor at zero.

**Cold and woken are identical, to the point.** Two full passes, nine
acquisitions each, the same nine. Consistent with the sweep's own note
that this provider was never cold. Whatever Monaco is doing, no amount
of contact wakes it, which is what `editor.accessibilitySupport = auto`
predicts: it is waiting for a screen reader and Lens is not one.

**The OCR rate of 16.9% is not a success rate**, and the report says so
in the output rather than only here. `word_at` answers only when the
point is inside a word, so that figure is the share of the window
covered by text. Accessibility's `RangeFromPoint` answers with the
nearest text from as far as 1,465px away, so its raw acquisition rate is
closer to asking whether a provider exists. The comparable pair is the
second table.

**Recognition cost 1.65s for a whole 1440x900 window.** That is the
instrument, not the product: `Ctrl+Alt+O` captures a 760x180 band, not a
window. It bounds the cost of a hypothetical whole-window read, and it
is why there is no such thing.

Measured immediately afterwards, in the product: the card offering the
fallback appeared once in VS Code, `Ctrl+Alt+O` over the editor read 44
characters, and the translation came back in 1.7s. That is the whole
path working in the application that prompted the milestone swap.

### Chrome OCR, 2026-09-01: a real-desktop OCR measurement

Build `5a11303`, `hover-probe ocr --budget 200`, Chrome on the secondary
monitor (1920x1080 at 100%), 252 grid points:

```
recognition  38 lines, 222 words, 1143 characters in 709ms
misread      5 of 222 words (2.3%) have a shape that cannot be right
median word  30x13px
grid         252 points, 35 of those on a word
OCR text covers 13.9% of the window
```

The 2.3% misread rate is how often `is_misread` fires on real content.
The other half of issue #86 (whether the words it stayed quiet about were
read correctly) needs a person.

Still open for the M4 gate: KakaoTalk (#34) with a visible conversation
window, and a real scanned PDF with a real file.

### Win32 `EDIT` across a process boundary, 2026-08-06

Issue #40 recorded that a plain `EDIT` read as `Empty` from inside its own
process and, between runs, as `Protected(Declared)` from another one. Both
answers are fail-safe, so it was filed as a matrix-correctness problem
rather than a privacy one, with three hypotheses and no explanation.

The explanation is neither UI Automation nor the control. **It is two DPI
coordinate spaces in one measurement.**

`ProtectedHarness::start` declares per-monitor DPI awareness. A test that
builds the harness and reads it is therefore aware on both sides and agrees
with itself. A test that spawns a child to build the harness is aware only
in the child: the parent asks UI Automation for rectangles and is handed
them divided by the display scaling, because a process that has not declared
awareness gets virtualised coordinates.

On this 125% display that is a factor of 0.8, and the arithmetic is visible
in the readings. The child reports its fields at `plain=(109,148 360x40)`
and `password=(109,208 360x40)`. The parent, asking about the plain field's
centre, was handed an element of `288x32`, which is `360x40` times 0.8, and
a background pane `1536` wide, which is 1920 times 0.8. The point computed
from the child's numbers landed inside the **password** field, which refused,
**40 times out of 40**. Nothing was ever wrong with the refusal.

The intermittency in the original report was test ordering: whether an
earlier test in the same binary had already built a harness and declared
awareness on the parent's behalf.

`UiaTextAcquirer::new` now declares it. Not as a courtesy: the acquirer's
contract is rectangles in physical screen pixels, and it cannot honour that
in a virtualised process, so it is a precondition of the type existing.

Measured after the change, one child process, same display:

```
plain EDIT, 40 readings from another process
  acquired               40
  element   control 50004, is_password false, (109,148 360x40), contains the point

password EDIT, 8 readings from another process
  protected               8
  element   control 50004, is_password true, (109,208 360x40), contains the point
```

Both rectangles now match the child's own report exactly, and both controls
answer the same way every time. The whole live suite passes on the corrected
coordinate space: **34 passed, 0 failed.**

Worth separating out, because it changes a row: the plain `EDIT` **acquired**
here, forty times. This repository's earlier note, inherited from Mori, was
that plain `EDIT` controls expose no text pattern at all. On this control, on
this build, they do.

### Two instruments on one pixel, 2026-08-06

The oldest open condition on the M1 gate is that **no person has confirmed
that a resolved word is the word they were pointing at**. Everything measured
before today used one proxy: whether the resolved word's rectangle contains
the sample point. That checks a position, not a word. A provider returning
the right rectangle around the wrong text passes it.

OCR is a second answer to the same question, arrived at from pixels rather
than from a provider, sharing no code and no assumptions with the first.
Where both resolve a word at one grid point they can be compared.

First run, `hover-probe ocr`, one Chromium window, 252 points, of which 89
had recognised text and 29 had a word from **both** instruments:

```
both instruments resolved a word at 29 points:
  the same word            21  72.4%
  one inside the other      3  10.3%
  different                 5  17.2%
```

"One inside the other" is a provider that expanded past the unit it was
asked for. It is not a wrong word and counting it as one would put the
expansion behaviour into the correctness figure.

**Seventeen percent disagree.** On this window, with these two methods, one
point in six where both could read returned different words. That is the
number the on-target proxy could never have produced, and it is large enough
to matter: a card placed on the wrong word explains the wrong thing.

The words are compared in memory and never printed
([`../privacy-model.md`](../privacy-model.md)).

**What this does not do is close the gate row.** Two methods agreeing is not
a person confirming, and both can be wrong the same way. What it does is
make the human pass cheap. Instead of judging every resolved word, there are
the disagreements, and if there is a wrong answer it is in them.

Still needed for that row: the same measurement over the named target
applications rather than one window, and a person looking at the
disagreements it produces.

## Known limitations, before measurement

Stated so a run does not rediscover them as bugs:

- An application running elevated cannot be read by a Lens running
  without elevation (UIPI). Lens does not run elevated
  ([`../privacy-model.md`](../privacy-model.md) §4).
- Games, video, images, and scanned PDFs expose no text and are expected
  to fail with `Unsupported`. That is the OCR fallback's territory (M5),
  not a defect.
- A protected field returns `Protected` and is correct behaviour.
