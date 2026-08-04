# ADR-0006 · No shared package with Mori yet

**Status** · Accepted, 2026-08-04

## Context

[ADR-0001](0001-independent-repository.md) separates the repositories.
This is the follow-up question: given two repositories that will
plausibly both need an Ollama client, a language-answer schema, and a
learning-item model, should a `mori-shared` package be extracted now?

The argument for is real. Writing an Ollama client twice is waste, and
if the two products ever exchange learning items they need a schema that
agrees.

The argument against is that neither interface is settled. Mori's
language answer is shaped by *what the user selected* (word, phrase,
sentence, paragraph) and is delivered as sections. Lens's quick
translation is shaped by *what the pointer rested on* and is deliberately
two fields ([ADR-0005](0005-two-speed-ai.md)). Those look similar and
are not, and a shared type extracted today would be the union of two
guesses. Worse, it would couple the release cadence of a shipping
application to a repository whose first milestone is a measurement
instrument.

The general rule: an abstraction extracted from one caller is a
hypothesis. Extracted from two callers that have both stabilised, it is
a fact.

## Decision

**No shared code with Mori until duplication is observed, in code, in
both repositories, against interfaces that have both stopped changing.**

Until then:

- Mori's verified findings come across as **documentation**, cited by
  file path in [`../research/inherited-from-mori.md`](../research/inherited-from-mori.md)
  and re-derived here. A measured fact about UI Automation is portable
  in a way a Rust module is not.
- Copying a small amount of code is preferred to sharing it, and the
  copy is noted with a comment naming the origin so a later extraction
  can find both sites.
- No `mori-shared`, no `mori-platform` monorepo, no git submodule, no
  path dependency.

If integration is wanted before that point, it is a **process boundary**
and not a library:

```json
{
  "schemaVersion": 1,
  "type": "language_learning_item",
  "sourceText": "...",
  "translatedText": "...",
  "sourceLanguage": "en",
  "targetLanguage": "ko",
  "focusedText": "produce",
  "createdAt": "..."
}
```

Carried over a localhost endpoint, a custom URL scheme, or an export
file, on an explicit user action. Lens never reads Mori's database and
never assumes Mori is running.

## Consequences

- Some code is written twice. The Ollama client is the likely case, and
  it is a few hundred lines against a documented HTTP API.
- Both products stay free to change their internal shapes without
  coordinating a release.
- The extraction, when it happens, is informed by two real
  implementations instead of one imagined interface.
- The risk this accepts: the two drift far enough apart that extraction
  never becomes worthwhile. That outcome is acceptable, because it would
  mean the shared package was never justified.
