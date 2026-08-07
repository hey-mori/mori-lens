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

## And then: the text is there, but not in lines

The paragraph below says the likely diagnosis is targeting rather than
reach. That is still right about the text being present and **wrong
about how easy it would be to reach**, and the number that says so was
measured an hour later.

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

That is the shape [#34](https://github.com/swarea/mori-lens/issues/34)
describes for KakaoTalk, in different words: *one text region, so Lens
cannot tell which message you are on*. Two applications, one problem,
which nothing before this could have shown.

### The first version of this measurement said 100% and meant nothing

It asked whether a point was inside **any** text element. Every window
answered 100.0%, because one element covers the whole client area and
containment is then trivially true. Recorded because the number looked
like a triumph and was an artefact of the question.

**It does not change the sweep.** Every point-based number in the matrix
stands. Nothing here shows a pointer resolving into an editor line,
because the window was covered and `ElementFromPoint` answers about
whatever is on top. That measurement still needs VS Code in front, and
it is now worth running again for the first time since the mode was
identified.

## Settled: it is the document

The paragraph above used to say this was the next measurement. It was
run the same afternoon.

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

## Why this took so long to notice

The condition was in the issue and in the reader's status bar, and the
instrument that could see it was the one nobody had pointed at a covered
window. The sweep reports `0/0 (N occluded)` and says nothing; the
inventory reports counts and says nothing about whether those counts are
the editor.

The comparison between the two is what carries the information, and it
did not exist until today.
