# ADR-0014 · Three tiers of history

**Status** · Accepted, 2026-08-27 (proposed 2026-08-13)

## Context

"Should Lens keep a history" has been one question with one answer, and the
answer has been no. `docs/privacy-model.md` says there is **no automatic
transcript of what was hovered**, and the translation cache is in memory and
dies with the process for that reason and no other.

But the same document also says a durable cache "is a **separate opt-in with its
own decision**", which defers rather than forbids, and `README.md` has said
`Storage (later): SQLite, **opt-in history**` the whole time.

So the prohibition is on two words together: **automatic** and **durable**.
Neither alone is the rule, and treating them as one has cost something real. A
reader who dismisses a card by moving the pointer, then wants it back, has no
way to get it, because the only thing that could have held it was refused under
a rule about permanence.

## Decision

**Three tiers, decided separately, with different defaults.**

| Tier | Default | Lives until | What it is for |
|---|---|---|---|
| Session recent | **on** | the process exits | finding what you just saw |
| Saved library | explicit save | deleted | what a reader chose to keep |
| Persistent history | **off** | the reader's choice | every translation, for a while |

### Session recent

Memory only, so it never touches the durable-record rule at all. It is the
translation cache with an interface, and the cache already exists.

- the latest **50 cards or two hours**, whichever prunes first
- consecutive answers for the same target are **deduplicated**, because a reader
  nudging the pointer along one sentence produced one intention and not eight
- it is gone when the process is, and nothing writes it anywhere

Two limits rather than one because they fail differently: fifty is what a list
stops being useful past, and two hours is what stops a machine left on overnight
from offering yesterday as though it were recent.

### Saved library

Unchanged. Explicit save, durable, and
[ADR-0013](ADR-0013-the-local-library.md) says where it lives.

### Persistent history

**Off by default, and off means nothing is written.** Not written and hidden.
Not written and pruned. Not written.

When a reader turns it on:

- retention **30 days** by default, chosen from 1, 7, 30, 90 days or forever
- at most **1,000 entries** by default
- **per-application exclusions**
- **the source application's name is not stored by default**, because a list of
  which programs somebody reads in is a different fact from what they read
- OCR and external-provider provenance stay visible on every entry, so a reader
  reviewing their own history can still tell a guess from a reading
- turning it on **does not retroactively persist** session recent, which was
  collected under a promise that it would not survive the process
- promoting an entry to the library creates a durable item that **survives
  history expiry**, because promotion is the explicit act the library is for

## Consequences

**`docs/privacy-model.md` changes**, and this is the change it has been deferring
since it was written. The page will say that Lens **can** be configured to keep a
transcript, that it does not by default, and what turning it on means, in the
reader's words rather than in a settings key.

**A product that can keep a transcript is a different product from one that
cannot**, even when the setting is off. That is the real cost of this decision
and it is worth stating rather than absorbing: the promise stops being *Lens
cannot* and becomes *Lens does not, unless you ask, and here is exactly what
asking does*.

**The defaults are the design.** Every number above can be argued with, and the
one that must not move without a new record is `off`.

**Nothing here is measured.** No reader has wanted a history, because no reader
outside this machine has used the product. The session-recent limits in
particular are chosen from what a list is like rather than from what anybody
reached for, and the honest test is a reader who turns persistent history on and
can afterwards say what they used it for. If they cannot, the setting only
creates risk and the right answer is to remove it.
