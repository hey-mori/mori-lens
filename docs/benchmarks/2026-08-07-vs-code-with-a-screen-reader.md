# VS Code with screen-reader mode on, 2026-08-07

[#14](https://github.com/swarea/mori-lens/issues/14) has said since the
first sweep that VS Code's editor exposes nothing to accessibility, and
has named one untested condition:

> repeat it with `editor.accessibilitySupport` turned on

The reader's own VS Code has been in that mode all along. Its status bar
reads **Screen Reader Optimized** in the screenshots that prompted this.

So the untested condition is the one that has been running, and three
instruments now disagree with the row on file.

Build `47b8101`. The window was **covered by another** for the whole of
this, and nothing was rearranged.

## What the tree holds

```
application          elements  withtext    chars  smallest  on-target
Code                      107       107     8131      16px      12/12
Code                       37        37     1367      11px      12/12
Code                       28        28      855      16px      12/12
```

107 text elements, **every one of them returning characters**, a
smallest of **16px**, and 12 of 12 sampled elements resolving back into
themselves.

16px is a line of code, not a window. A window holding its text in one
piece reports a smallest the height of itself, which is what KakaoTalk
does ([#34](https://github.com/swarea/mori-lens/issues/34)). This is the
opposite shape.

## What the pixels hold, and how much of it is in the tree

`hover-probe ocr --only Code`, which now renders a covered window rather
than requiring it in front:

```
rendered  the window is behind another; it drew itself, 99.5% ink.
by handle 216 distinct words on the pixels, 140 of them also in the
          accessibility tree (65%). The tree holds 653 distinct words in all.

recognition  99 lines · 389 words · 2228 characters
```

**Two thirds of what is on the screen is in the tree**, and the tree
holds three times as many distinct words as the pixels show, which is
what a scrolled document looks like from the inside.

Counts only. Neither reading is printed, and the tree's words are held
as a set so they cannot reconstruct a document
([`../privacy-model.md`](../privacy-model.md)).

## What this changes, and what it does not

**It changes the diagnosis.** The row on file says the editor exposes
nothing. On this machine, in this mode, it exposes a great deal, in
per-line elements, and 65% of it can be matched word for word against
the pixels. The remaining 35% is bounded from below by OCR misreading,
which the same session measured doing `is` to `15` and `for` to `于or'`.

So the problem for VS Code is very likely **targeting rather than
reach**: the text is in the tree and `ElementFromPoint` at a coordinate
does not land on it.

**It does not change the sweep.** Every point-based number in the matrix
stands. Nothing here shows a pointer resolving into an editor line,
because the window was covered and `ElementFromPoint` answers about
whatever is on top. That measurement still needs VS Code in front, and
it is now worth running again for the first time since the mode was
identified.

**It does not identify which elements hold which words.** 107 elements
and 653 words is consistent with the editor being exposed and also with
a file tree, a tab strip and a command palette being unusually wordy.
Pairing a word with the element that holds it is the next measurement
and it is not this one.

## Why this took so long to notice

The condition was in the issue and in the reader's status bar, and the
instrument that could see it was the one nobody had pointed at a covered
window. The sweep reports `0/0 (N occluded)` and says nothing; the
inventory reports counts and says nothing about whether those counts are
the editor.

The comparison between the two is what carries the information, and it
did not exist until today.
