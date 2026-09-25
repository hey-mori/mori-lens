# Nothing reaches disk, 2026-08-18

`capture.rs` opens with the strongest promise in this project:

> **Nothing reaches disk. Ever, including for debugging.**

`CONTRIBUTING.md` says it twice and M4's gate asks for it. Until today the sentence
appeared in this repository as a module comment, as a roadmap row, and as a
**translation fixture in another crate's live tests**. Nowhere as a check.

`protected_harness` was built on the same reasoning, and its comment is the one
worth repeating: a privacy guarantee nobody has run is a comment.

## Both halves

**The construct is refused.** `scripts/check.sh` fails if `capture.rs` or
`ocr_acquire.rs` mentions `fs::`, `File::`, `CreateFile`, `WriteFile`,
`BitmapEncoder` or `SaveToStream`. Checked against a planted
`std::fs::write` and it fails on it.

A check that watches for a file appearing can only fail **after** one has
appeared. This one fails on the way to writing it.

**The result is watched.** `an_ocr_acquisition_writes_nothing_to_disk` takes
real screenshots and looks at where they could have gone:

```
captures taken: 2
C:\Users\Admin\AppData\Roaming\Mori Lens · 2 files, 0 new
C:\dev\mori-lens\crates\platform-windows · 1 files, 0 new
redirected temp · ...\mori-lens-disk-watch\30484 · 0 files
```

## Two versions of this test were worthless, and both passed

**The first watched the system temporary directory.** It saw 38 files appear
during one run, every one a GUID with a `.tmp` suffix, none attributable to
anything. It then checked their extensions for `.png` and `.bmp`, which a
capture spooled as `.tmp` would have sailed through.

The fix is redirection rather than filtering: `TMP` and `TEMP` are pointed at an
empty directory before the acquirer exists, so anything written there was
written by this process.

**The second could not tell whether a capture happened.** Its guard read
`captured > 0 || !points.is_empty()`, whose right half is always true. It passed
a run where both points returned `Empty` and no screenshot was taken at all.

The fix is to ask the counter the product already keeps: `CaptureCost::captured`
is screenshots taken, not readings that found text, and the assertion is on
that.

**Both of those runs were green.** That is the entry worth keeping from this
benchmark: on a promise this size, a passing test is not evidence until you can
say what would have made it fail.

## What this does not cover

- **A write anywhere else on the filesystem.** Two directories are watched and
  the temporary path is redirected. Nothing looks at the rest of the disk.
- **A component that resolved the temporary path before the redirection**, or
  that hardcodes one. The redirection is read by anything asking Windows at the
  time, and not by anything that asked earlier.
- **The other paths that touch pixels.** `PrintWindow` in the inventory probe
  and the overlay's own rendering are not covered by the source guard, which
  names two files.
- **A capture held in memory too long**, which is a different promise and has no
  check here.
