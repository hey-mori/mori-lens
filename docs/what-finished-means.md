# What finished means

[`mori-lens-project-goal.md`](mori-lens-project-goal.md) says that what is
still missing before this is something a person installs is **a signed
binary**, and that it is the whole of the remaining list. That is true,
and this document is about a different distance: from **installable** to
**finished**.

Nothing on the first list is on this one. A signed binary makes the
product safe to hand over. It does nothing about whether the person
holding it ever gets a card.

The roadmap stages the engineering. This states when the engineering is
enough, which is a different question and has never been written down.

## The three things in between

### 1. Nobody has performed this gesture before

Every tool this reader has used trains the same reflex: **select, then
act**. Highlight and press a shortcut. Right-click and choose. Copy and
paste into a box. Twenty years of software has taught one lesson about
getting a machine to look at text, and Lens asks for the opposite of it.

Lens asks them to **stop moving**. That instruction has no precedent, it
is invisible, and it is indistinguishable from the product being broken.
A reader who does not know the gesture and a reader whose window is
unsupported see exactly the same thing: nothing.

There is no first-run experience. `Card::hold_the_modifier` looks like
one and is not: it explains a **tier**, to somebody already hovering, who
has therefore already discovered the gesture this product needed to
teach.

**A product whose entire premise is a novel gesture, and whose answer to
the first thirty seconds is nothing, has not shipped its premise.** It
has shipped the machinery under it.

The fix is not a tutorial. It is that **the first hover a person ever
completes must be arranged rather than hoped for**, in a window this
repository controls and has measured, on a sentence it wrote itself, so
that the first card is guaranteed rather than probable.

### 2. The first card is several gigabytes away

Ollama, then a model. Neither is bundled and neither will be:
[ADR-0007](decisions/ADR-0007-local-first-inference.md) is the reason
this product can promise that nothing leaves the machine, and bundling a
runtime would not change the size, only who is surprised by it.

So this distance cannot be removed. It can only be **shaped**, and
shaping it is product work rather than engineering work.

What exists today is correct and is not a plan: an absent Ollama names
ollama.com, a missing model names the `ollama pull` command, on the card
and in the tray tooltip. That is the right behaviour for a reader at
minute twelve who has hit a wall. It is not an answer for a reader at
minute zero who has not started.

**The number that decides this has never been measured:** how long from
unzipping to the first card, on a machine with neither Ollama nor a
model, timed on somebody who did not build it. Every other figure in
this repository is about a window. This one is about the product, and it
is the only one that predicts whether a stranger ever sees it work.

### 3. Nothing brings a reader back

M8 saves. Nothing returns.

A tool that helps someone read a sentence they would otherwise have
skipped, and then never mentions it again, is a utility. A utility is
used when it is remembered, and a tray icon is not a reminder.

This is the least urgent of the three and the one that decides whether
the product is kept after the week it is interesting.

## What v1 is

**One job:** you rest the pointer on a sentence you cannot read, and you
can read it, without touching anything and without the sentence leaving
the machine.

Everything shipped serves that sentence. The deep path explains a word
in it, OCR reaches it when accessibility cannot, the distance gate
refuses to answer about a different sentence, and the privacy model is
the second clause of it.

**v1 refuses,** and the refusals are the shape of the product rather
than a shortage of time:

- It does not read on a timer, an event, or a mouse move.
- It does not send text off the machine.
- It does not act. It shows, and the reader decides.
- It does not ask to be trusted with a screen it cannot prove is
  unlocked. Unreadable is treated as secure.

## The gates that are missing

Every gate condition in [`roadmap.md`](roadmap.md) is a **measurement**:
an acquisition rate, a latency, a window in a matrix. That is right for
a repository that calls itself a measurement instrument before it is a
product, and it is why the compatibility figures here are trustworthy.

**Not one of them is about a person.** So the roadmap can go entirely
green while nobody outside this machine has ever seen a card.

Three gates belong at the end, phrased as a person rather than a number:

1. **Someone who did not build this installs it unaided and gets a
   card.** The time is recorded, and the recording is the measurement
   §2 says is missing.
2. **They keep it running for a week without being asked to**, and can
   say afterwards what it was useful for.
3. **They can state what it is for in one sentence**, and that sentence
   matches the one under *What v1 is*. If it does not, the product is
   explaining itself wrongly, which is a defect in the product and not
   in the reader.

These are unfalsifiable by a test suite and cannot be run here, which is
exactly why they have to be written down. The gates that can be
automated already were.

## What this document is not

It is not a schedule, and it does not add a milestone. M6 through M8
stand as they are.

It is not permission to build any of it. Each of the three is its own
issue, its own branch, and its own measurement, under the same rule as
everything else here: nothing is claimed to work on the strength of a
green test run.
