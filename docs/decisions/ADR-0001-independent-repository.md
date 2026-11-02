# ADR-0001 · Mori Lens is an independent repository

**Status** · Accepted, 2026-08-04

## Context

Mori Lens started as an idea for a Mori feature: Mori already ships a
language capability that reads another application's *selection* through
UI Automation and explains it in a mini overlay
(`hey-mori/mori`, `src-tauri/src/language/`, documented in that repository's
`docs/design/language-loop.md`). Hover-to-understand looks like the next
adapter on that same ladder.

Three facts argue against building it there.

1. **Mori is expensive to run.** It is a Tauri application with a React
   frontend, a SQLite store, seven agent readers, and an import pipeline.
   The thing Lens has to prove is a 300ms interaction loop. Iterating on
   that loop behind a full application build means paying Mori's startup
   and rebuild cost on every measurement, and Mori's
   `docs/design/language-loop.md` §7 already records one round of "the
   fix does not work" caused by testing against a binary that predated
   the fix.
2. **The runtime shapes disagree.** Mori is a windowed application the
   user opens. Lens is a tray-resident process that watches a pointer and
   must be cheap at idle, must not steal focus, and needs accessibility
   and later screen-recording permissions that Mori does not want to ask
   for.
3. **A long-lived feature branch is the worst of both.** It inherits
   Mori's dependency tree and build time, cannot release on its own,
   widens Mori's test surface, and grows unmergeable as `main` moves.

A monorepo is the other candidate. It is the right answer when two apps
genuinely share code and a release cadence. Today they share neither, and
adopting it now would couple two products whose interfaces are both
unsettled.

## Decision

**Mori Lens lives in `hey-mori/mori-lens`, an independent repository, and
runs with Mori not installed.**

Concretely:

- No dependency on Mori's runtime, database, schema, build, or UI.
- No import of Mori source. Mori is *reference material*: its verified
  findings are cited in [`../research/inherited-from-mori.md`](../research/inherited-from-mori.md)
  with a file path, and re-derived here.
- Its own settings, its own storage, its own update path.
- Integration, if it ever happens, is a documented process boundary
  (a local endpoint or an export file), never a shared library. See
  [ADR-0006](ADR-0006-no-shared-package-yet.md).

## Consequences

- Some code will genuinely be written twice, most likely the Ollama
  client and the prompt scaffolding. That is accepted, and it is the
  cheaper mistake: an abstraction extracted from one caller is a guess,
  and a wrong guess couples two products permanently.
- Lens carries its own workflow, CI, and release surface. Duplication is
  reduced by copying Mori's conventions verbatim rather than by sharing
  infrastructure.
- A future merge into a `mori-platform` monorepo stays possible, because
  nothing here assumes a repository root. Doing it requires evidence that
  the two products actually share code, not the expectation that they
  might.
