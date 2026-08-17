# ADR-0013 · The local library is SQLite, and the file survives it

**Status** · Proposed, 2026-08-13

## Context

`Ctrl+Alt+S` appends a dated block to `saved.md` and **nothing reads it back**.
`keep.rs` says so directly, and gives a reason that is not taste:

> Because the gate's other two clauses are *export round-trips* and *deletion is
> real*, and a file somebody can open in any editor satisfies both without code.
> Export is the file. Deletion is selecting a block and pressing delete, which is
> more real than a button that sets a flag.

That argument is sound, and it is about **durability**. It says nothing about
structure, and structure is what M8's other three words need: *saved words and
sentences, **notes, review list, export***. One of the four is built.

`README.md` has meanwhile carried `Storage (later): SQLite, opt-in history`
since before any of this was discussed. A proposal that a database should wait
for a measurably slow file was written on 2026-08-13 without noticing that line,
which is [#160](https://github.com/swarea/mori-lens/issues/160)'s whole subject.

**The test "wait until reading the file is slow" is the wrong test.** It measures
the one requirement a flat file meets. The requirements it does not meet are
structural and are as true at ten items as at ten thousand:

- stable item identity that survives an edit
- original, translation, deep analysis, notes and tags as separate fields
- more than one answer for the same sentence, from different models
- provenance: provider, model, prompt and schema version
- search and filters, and full text search
- review state and scheduling
- duplicate detection
- atomic edit and delete
- migration with a schema version
- transfer status for the Mori boundary
  ([ADR-0006](ADR-0006-no-shared-package-yet.md))

## Decision

**`library.db`, SQLite, is the structured source of truth. `saved.md` is not
replaced by it.**

### The file survives

- **`saved.md` is never overwritten and never removed.**
- On first activation of the library it is **imported best effort**. What cannot
  be parsed is left alone and reported, not discarded.
- Afterwards it becomes the **human-readable export**, which is what `keep.rs`'s
  argument was protecting all along.
- A **versioned JSONL export** carries the lossless round trip, because Markdown
  is a rendering and not a serialisation, and the M8 gate says export
  round-trips.

### What lives where

- Content in `library.db`.
- Settings stay their own file, outside it.
- **API keys and tokens go to Windows Credential Manager and the macOS
  Keychain**, never to the database and never to the settings file.

### No database encryption in v1

Encrypting it would put the key on the same machine as the database, which
defends against a threat this product does not face and complicates one it does:
a reader who can no longer open their own library.

What is used instead is the operating system's own file permissions, and the
threat model is stated rather than implied:

- **Another account on this machine** · addressed. The file is the reader's.
- **Malware running as the reader** · **not addressed.** It can read the
  database exactly as it can read `saved.md`, the settings file, and the screen.
- **Somebody with the disk** · not addressed. That is full-disk encryption's
  job and the operating system's.

### What deletion means, stated per store

**A plain SQLite `DELETE` is not erasure.** Pages are freed and reused and the
content can remain in the file. So:

- **Deleting an item** removes the row, its full text search entry, and any
  derived cache. It is described as deleting an item and not as erasing bytes.
- **"Delete all data securely"** is a separate action: a WAL checkpoint, then
  `secure_delete`, then `VACUUM`. Those do remove the residue **inside the
  SQLite file** and that is a real thing they do.
- **What no button in this product reaches**: filesystem snapshots, whatever the
  drive's controller did with the old blocks, any backup, and any export the
  reader made. Each is named in the interface rather than covered by a promise
  that sounds larger than it is.

## Consequences

**`docs/privacy-model.md` changes here**, and deliberately. It currently says
nothing that came off a screen is written except `saved.md`, and that sentence
was corrected on 2026-08-13 after four days of being wrong in the other
direction. The amendment is argued in this record rather than applied quietly,
and it says what a reader can check.

**`keep.rs`'s reasoning survives, in the role it was really defending.** Export
is still a file anybody can open. Deletion is still something a person can do
with an editor, on the export, and now also something the product does properly
on the store.

**Migration is the risk.** A reader with a year of `saved.md` has a file this
product has never parsed, containing whatever they typed into it, because
nothing reads it back and so nothing could break by writing in it. The import is
best effort by necessity, and the file being untouched is what makes a failed
import survivable.

**It does not add sync, conflict resolution, or any field that exists only for
a server.** [#169](https://github.com/swarea/mori-lens/issues/169) gates that,
and a schema that anticipates it would be a schema shaped by a decision nobody
has taken.

**It stores nothing a reader did not keep on purpose.** Whether a durable
history exists at all is [#163](https://github.com/swarea/mori-lens/issues/163)
and its own record. This ADR gives that decision somewhere to put its rows and
takes no position on whether it should.

**Nobody has asked for this either.** No reader outside this machine has saved
an item, so the ten requirements above are derived from what a library is rather
than from what anybody has wanted. That is weaker evidence than this repository
usually acts on, and it is the same caveat ADR-0012 carries.
