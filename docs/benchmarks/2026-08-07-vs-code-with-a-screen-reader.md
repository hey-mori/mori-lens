# VS Code with screen-reader mode on, 2026-08-07

[#14](https://github.com/hey-mori/mori-lens/issues/14) has said since the
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
does ([#34](https://github.com/hey-mori/mori-lens/issues/34)). This is the
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

## Settled: it is the document

The counts above cannot say whether 8,131 characters are the document or
the file tree beside it. This is the measurement that can, run the same
afternoon.

`hover-probe inventory --only Code --expect <word>` asks the tree
whether it holds a word the operator names. Printing is allowed here and
only here, because the words come **from** the operator: what is
reported is whether something already known was found, which is the
exception `../privacy-model.md` names.

First attempt, symbol names from the open file:

```
Code   2 of 5 expected words, 648 distinct words in all
    show_word              in the tree
    interval               in the tree
```

Not decisive. VS Code's Outline view lists a struct's fields, so a
symbol being in the tree could be the sidebar rather than the editor.

Second attempt, **prose from a doc comment**, which an outline never
holds and a file tree cannot:

```
Code   6 of 6 expected words
    hand                   in the tree
    elsewhere              in the tree
    dependency             in the tree
    correctness            in the tree
    judgement              in the tree
    requiring              in the tree
```

Every one. Those six words appear together in one `///` comment in
`apps/hover-probe/src/probe/mod.rs`, which is the file the editor was
showing. A file tree holds filenames, a tab strip holds filenames, and
an outline holds symbols. **The document is in the tree.**

The two other VS Code windows on the same desktop answered 0 of 6, which
is what a different file open in a different window should do and is a
check that the question is discriminating rather than always saying yes.

## And then: the text is there, but not in lines

Knowing the document is in the tree, I read the diagnosis as *targeting*
and nearly solved. That is right about the text being present and
**wrong about how easily it could be reached**, and the number that says
so was measured an hour later.

`hover-probe inventory --coverage` lays the sweep's own 24px grid over a
window and asks, for each point, whether the **smallest** text element
containing it is line-shaped.

```
window                 client        points  on a line              covered
Code                    1549x908       2432      54 ( 2.2%)        2432
Code                    1440x900       2220      51 ( 2.3%)        2220
Code                    1440x900       2220     306 (13.8%)        2220
```

**2.2%, against the sweep's 2.8% acquisition.** Those two tracking each
other is the finding. The product acquires almost exactly where a
line-shaped text element is, and almost nowhere else.

So the 8,131 characters are in the tree and they are **not** laid out as
one element per line. They are in something large, and a point inside
something large does not say which sentence is under the pointer.

That is the shape [#34](https://github.com/hey-mori/mori-lens/issues/34)
describes for KakaoTalk, in different words: *one text region, so Lens
cannot tell which message you are on*. Two applications, one problem,
which nothing before this could have shown.

### The first version of this measurement said 100% and meant nothing

It asked whether a point was inside **any** text element. Every window
answered 100.0%, because one element covers the whole client area and
containment is then trivially true. Recorded because the number looked
like a triumph and was an artefact of the question.

## What none of this changes

**The sweep.** Every point-based number in the matrix stands. Nothing
here shows a pointer resolving into an editor line, because the window
was covered for all of it and `ElementFromPoint` answers about whatever
is on top. That run still needs VS Code in front, and it is worth doing
again for the first time since the mode was identified.

**The 65% is bounded from below by OCR.** The same session measured this
recogniser turning `is` into `15` and `for` into `于or'`, so a word the
tree does not have is as likely to be a misreading as an absence.

## What does change

The row on file says the editor exposes nothing. **It exposes a great
deal**, and no amount of getting OCR right was ever going to be the
answer for it.

What it does not expose is one element per line. That was the shape this
document assumed for an hour, on the strength of `smallest 16px` in the
inventory, and the grid measurement above is what corrected it: 16px
elements exist, and they are not what the editor's text is in.


## And then: walking up does not reach it either

The obvious response to *the text is in a container* is to walk up from
the point until something has a text pattern. That was measured the same
day. The first attempt measured it wrongly, testing the degenerate range
`RangeFromPoint` returns rather than one expanded to a line; corrected
and re-run across the whole desktop, the answer got worse rather than
better. Of 17 points where an ancestor had a pattern, **none** returned
a line containing the point.

[`2026-08-07-walking-up-from-the-point.md`](2026-08-07-walking-up-from-the-point.md).

## Why this took so long to notice

The condition was in the issue and in the reader's status bar, and the
instrument that could see it was the one nobody had pointed at a covered
window. The sweep reports `0/0 (N occluded)` and says nothing; the
inventory reports counts and says nothing about whether those counts are
the editor.

The comparison between the two is what carries the information, and it
did not exist until today.
