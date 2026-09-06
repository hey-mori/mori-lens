# ADR-0015 · The library schema

**Status** · Accepted, 2026-08-31

## Context

[ADR-0013](ADR-0013-the-local-library.md) decided that SQLite is the
structured source of truth, and that `saved.md` survives as the
human-readable export. What remained was the schema itself: which fields
the `library` table carries, how migration is tracked, and how search
works.

The current save flow writes four fields to `saved.md`: original text,
answer text, an optional caveat, and an optional application name.
[ADR-0013](ADR-0013-the-local-library.md) lists ten requirements a flat
file does not meet, including stable identity, separate fields for notes
and tags, provenance, search, and review state. The schema must make
room for those without over-fitting to requirements that no reader has
asked for yet.

## Decision

**One table, versioned migrations, LIKE search.** The schema is small
enough to state here in full.

### The library table

```sql
CREATE TABLE library (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    original        TEXT    NOT NULL,
    answer          TEXT    NOT NULL,
    caveat          TEXT,
    app             TEXT,
    created_at      INTEGER NOT NULL,
    kind            TEXT    NOT NULL DEFAULT 'translation',
    source_language TEXT,
    target_language TEXT,
    model           TEXT,
    note            TEXT
);
```

- `id` · assigned by the database. Stable across edits; the M8 gate's
  *stable item identity that survives an edit*.
- `original` · the text on the screen, as the card showed it.
- `answer` · the translation or explanation, as the card showed it.
- `caveat` · the AI-explanation label, when the answer is a deep
  analysis. NULL for translations.
- `app` · the source application's process name. NULL when unknown.
- `created_at` · seconds since the Unix epoch, UTC. The same clock
  `keep.rs` uses for the Markdown heading.
- `kind` · `"translation"` or `"analysis"`. Not an enum in the database;
  a plain string, because adding a kind is a migration and a string
  costs nothing.
- `source_language`, `target_language` · BCP 47 codes. NULL when the
  save flow did not capture them. Reserved for the library UI and for
  filters.
- `model` · the model that produced the answer. NULL when unknown.
- `note` · the reader's own words. NULL until a reader writes one.

Indexes on `created_at DESC` and `kind` for the two queries the library
UI runs first: newest-first listing, and filter-by-kind.

### Search

LIKE with `%query%` on both `original` and `answer`. Not FTS5, for
three reasons:

- The corpus is a personal library, not a search engine. LIKE on a few
  thousand rows is microseconds.
- FTS5 with the default tokenizer cannot search CJK text, which is the
  product's primary language pair. A custom tokenizer is its own crate
  and its own maintenance burden.
- LIKE works uniformly across all scripts. No language detection, no
  tokenizer selection, no per-query branching.

If the library grows past what LIKE serves comfortably, FTS5 with a
custom tokenizer replaces the WHERE clause and nothing else.

### Migration tracking

```sql
CREATE TABLE schema_migrations (
    version    INTEGER PRIMARY KEY,
    applied_at TEXT NOT NULL DEFAULT (datetime('now'))
);
```

Each migration inserts its version number. On open, the library reads
the table and applies every version that is not yet recorded. Migration
1 creates the schema; migration 2 adds a `meta` table for import
tracking.

### The meta table

```sql
CREATE TABLE meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
```

Used to track whether `saved.md` has been imported. The key
`'imported'` with value `'true'` means the import has run.

## Consequences

**The crate is independent.** `lens-storage` has one dependency
(`rusqlite` with `bundled`) and no dependency on `lens-core`. The item
type is defined locally, and the app-level code maps between
`keep::Entry` and `storage::Item`. This is what makes the storage crate
testable in isolation and what keeps `lens-core` free of OS crates.

**LIKE is the search, and it is honest about what it is.** A substring
match is not a ranked full-text search, and the product does not claim
it is. For a personal library, it finds what a reader types, in any
language, with no configuration.

**The schema has room for notes, tags, and review state.** `note` is in
the schema because the M8 gate asks for it. Tags arrived in migration 3
as a many-to-many relationship. Review state arrived in migration 4 with
`review_state`, `next_review_at`, and `review_count` columns and a
partial index on `next_review_at` for the due-items query.

**Migration 7 replaces spaced-repetition scheduling with manual review.**
The review state values are now `"known"`, `"later"`, and `"archive"`.
The `next_review_at` and `review_count` columns remain in the schema
(added by migration 4) but are no longer written by the application.
Migration 7 remaps `"new"` and `"learning"` to `"later"`, `"mastered"`
to `"known"`, drops the partial index on `next_review_at`, and adds an
`archived_at` column. The reader opens the Library and picks what to
review; no timers, no notifications, no automatic scheduling.

**`saved.md` is still written on every save.** The database is a second
store alongside the file, not a replacement. If the database write
fails, the file write has already succeeded. The file is the safety net
that was there before the database existed, and ADR-0013 says it is
never removed.

**The kind is inferred from the caveat.** An entry with a caveat is an
analysis; one without is a translation. This is a convenience for the
first version. When the save flow carries the kind explicitly, the
inference becomes unnecessary.
