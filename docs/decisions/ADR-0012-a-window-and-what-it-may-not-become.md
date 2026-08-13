# ADR-0012 · A window, and what it may not become

**Status** · Proposed, 2026-08-11

## Context

Lens has no window. It has a tray icon, a settings file somebody edits in
Notepad, a passive card that cannot be clicked
([ADR-0004](ADR-0004-passive-overlay.md)), and a welcome that appears once
and is never seen again.

Everything the product has learnt about a reader goes into one of two
places: `settings.conf`, which they typed, and `saved.md`, which they
appended to one keypress at a time. **Nothing reads either back.**

That is now the shape of three separate gaps:

- **M8's review list.** The milestone says *saved words and sentences,
  notes, review list, export*. Saving is built. The review list is not,
  and it has nowhere to live.
- **[#141](https://github.com/swarea/mori-lens/issues/141).** A tool that
  helps somebody read a sentence and then never mentions it again is a
  utility, and a utility is used when it is remembered. A tray icon is
  not a reminder.
- **Settings.** The reader chooses a model, a language, a theme, and a
  per-application rule by editing a text file. That is a good file and it
  is not a way to choose a model.

So: should Lens have a window?

## Decision

**Yes, and its boundaries are the point of this record.**

### What the window is

One window, opened from the tray, holding three things:

1. **What you kept.** A reader of `saved.md`, in the order it was
   written, searchable.
2. **What Lens is set to.** The settings file, shown as controls: the
   language, the model chosen from what Ollama actually has, the theme,
   the dwell, the per-application rules, and whether OCR falls through.
3. **Whether it is working.** The things the console says today and a
   release binary has no console for: which model answered, how long the
   last answers took, what the fall-through has cost
   ([#87](https://github.com/swarea/mori-lens/issues/87)).

### What it may not become, and why each is a rule rather than a preference

**Not a second store.** `saved.md` stays the store, and the window is a
**reader** of it. `keep.rs` records why the file is plain Markdown, and
the reason is not taste: the M8 gate says *export round-trips* and
*deletion is real*, and a file somebody can open in any editor satisfies
both without code. Export is the file. Deletion is selecting a block and
pressing delete, which is more real than a button that sets a flag.

A database would move both properties into code that has to be trusted,
and would take the reader's own notes with it, since nothing parses that
file back today and so nothing can break by writing in it.

**If the file ever gets too large to read, the answer is an index beside
it and not a store instead of it.** An index is derived, rebuildable, and
deletable without losing anything. That distinction is the whole of it,
and a change that blurs it needs its own ADR.

**No accounts.** They are on the roadmap's exclusion list, beside cloud
sync, and that list says adding one is a decision rather than a task.
This is that decision, and the answer is no, because an account is a
thing an account is *for*: it identifies a person to a server. Lens's
whole claim is that nothing goes to a server
([ADR-0007](ADR-0007-local-first-inference.md)), and a product that asks
who you are while promising it never phones home is asking a reader to
hold two beliefs that cannot both be true.

**No sync.** Same list, same reason. Two machines with two `saved.md`
files are two files, and a reader who wants them merged has a folder that
syncs, which is theirs and not this product's.

**Not the card, and never in the pointer's way.** ADR-0004 makes the card
passive and click-through, and none of that changes: this window is an
ordinary application window a reader opens deliberately, and closing it
leaves the product exactly as it is today. **The tray is still how Lens
runs**, and the window is not required for a hover to work.

**Nothing new is recorded.** The window shows what already exists. It
does not add a history of what was hovered, which would be the durable
record of what somebody read that `docs/privacy-model.md` refuses, and
which the translation cache is deliberately in-memory to avoid.

### And Mori

The roadmap already answers this: *a process boundary carrying learning
items on an explicit user action. Never a shared library, never a read of
Mori's database* ([ADR-0006](ADR-0006-no-shared-package-yet.md)).

A window changes nothing about that and makes it easier: an explicit user
action needs somewhere to be taken, and until now there was nowhere to
put the button.

## Consequences

**Lens becomes an application rather than a tray tool.** That is a real
change in what it is, and it is the cost of the three gaps above having
nowhere else to go.

**The settings file stays authoritative.** The window writes it, the same
file, in the same format, and a reader who prefers Notepad loses nothing.
Two ways to set one thing is a synchronisation problem, and the way out
is that one of them is a view of the other.

**It is a new platform surface.** A window is Win32 and then AppKit, and
`lens-core` must not learn about either
([ADR-0003](ADR-0003-rust-core-platform-adapters.md)). What belongs in
the core is what the window *shows*: the parse of the saved file, the
list of settable things and their valid values, and the session's own
figures. What belongs in the adapter is the window.

**It is not free.** This is the largest single addition since the
overlay, and it competes with the three things
[`../what-finished-means.md`](../what-finished-means.md) says stand
between this product and a stranger seeing a card. It should be built
after those and not instead of them.

**Nothing here is measured.** No reader has asked for this window, because
no reader outside this machine has used the product at all. It is
proposed on the strength of three recorded gaps, which is weaker evidence
than this repository usually acts on, and that is worth saying in the
record rather than discovering later.
