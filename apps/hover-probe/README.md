# hover-probe

Milestone 1's deliverable. It is an instrument, not a feature: it
follows the pointer, resolves what is under it through UI Automation,
and records what happened. It translates nothing and draws nothing
([ADR-0008](../../docs/decisions/ADR-0008-probe-before-product.md)).

## Run

```bash
cargo run -p hover-probe -- --auto
```

Rest the pointer on text. One line per resolved hover. Ctrl+C prints
the per-application summary.

Without `--auto` the probe follows the shipping default and only looks
while **Alt** is held. `--auto` exists so a compatibility run does not
need a third hand.

## Reading a line

```
chrome           ok                      14ms  word=7 sent=81  para=240  rects=1/2  dist=0  ctrl=50030  woke
│                │                       │     │      │        │         │          │       │          │
application      outcome                 how   word   sentence paragraph rectangles pointer UIA        needed the
                 or failure reason       long  chars  chars    chars     word/ctx   distance control    accessibility
                                                                                    from the type       wake
                                                                                    resolved text
```

`dist` is the one to watch. `RangeFromPoint` answers with the text
*nearest* the coordinate, so a large distance means the probe resolved
a sentence the pointer was not actually over. Whether a threshold is
needed, and where it belongs, is one of the questions this milestone
answers.

Failure lines carry the reason and nothing else, because a failed read
measured nothing:

```
notepad          unsupported              3ms
```

The reasons are distinct facts and are counted separately:
`unsupported`, `empty`, `element-gone`, `protected`,
`protected-unprovable`, `timeout`, `stale`, `unavailable`.

## Judging whether the right word was resolved

The probe prints no content by default
([privacy model](../../docs/privacy-model.md) §3). Two ways to judge
correctness anyway:

```bash
cargo run -p hover-probe -- --auto --expect produce
```

Say what is under the pointer; the probe appends `MATCH` or `MISMATCH`
and prints nothing. Prefer this.

```bash
cargo run -p hover-probe -- --auto --show-word
```

Prints the resolved word, and nothing wider. Never the sentence, never
the paragraph. It says so in the banner while it is on. Use it when
reading real prose, where naming every word in advance is not
practical.

## Options

| Flag | Default | Meaning |
|------|---------|---------|
| `--auto` | off | Hover everywhere, not only while Alt is held |
| `--dwell <ms>` | 320 | How long the pointer must rest |
| `--interval <ms>` | 25 | Pointer sampling period |
| `--expect <word>` | none | Report MATCH or MISMATCH without printing |
| `--show-word` | off | Print the resolved word only |

## Recording a run

A run is worth something only with its environment, because none of
these numbers are portable across machines:

```
date          2026-08-04
build         <git sha>
os            Windows 11 26200
scaling       150% primary, 100% secondary
monitors      2, secondary left of primary (negative X)
```

Hover over ordinary body text for a few seconds, then a heading, then a
single word, then near the edge of a line. Ctrl+C. Transcribe the
summary into
[`docs/compatibility/windows-app-matrix.md`](../../docs/compatibility/windows-app-matrix.md).

The gate for Milestone 2 is that matrix covering the named target set,
not an impression that the probe seems to work.

## What it cannot see

Stated so a run does not mistake a gap for a result.

- **Scrolling and typing are not detected.** Both are cancelling
  signals in the hover state machine and both are unit tested there,
  but detecting them needs an input hook the probe does not install.
  In a run, a hover during a scroll is not suppressed.
- **An elevated application cannot be read** by a probe running without
  elevation (UIPI). Do not run it elevated to work around this.
- **Games, video, images, and scanned PDFs** expose no text and are
  expected to fail with `unsupported`. That is the OCR fallback's
  territory (M5), not a defect.
