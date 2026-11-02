# ADR-0018 · The Mori process boundary

**Status** · Accepted, 2026-09-03

## Context

[ADR-0006](ADR-0006-no-shared-package-yet.md) separates the repositories
and says no shared code until duplication is observed in both, against
interfaces that have both stopped changing. If integration is wanted
before that point, it is a process boundary and not a library.

[#168](https://github.com/hey-mori/mori-lens/issues/168) names the scope:
capability discovery, schema version negotiation, idempotency, explicit
consent, graceful absence, and typed failure. The non-goals are: no
shared package, no reading from Mori, no coupling releases.

The user outcome: a reader who uses both products sends a learning item
from Lens to Mori on purpose, and a reader who uses only Lens never
notices Mori exists.

## Decision

**File handoff to a connector directory, with a versioned JSON payload.**

### Transport

Lens writes a JSON file to `~/.mori/connectors/lens/`. Mori reads files
from that directory. The directory pattern matches Mori's existing
connector inbox (`~/.mori/connectors/{name}/`), but the format is
purpose-built for learning items rather than agent session events.

Why file handoff over local HTTP:

- no HTTP server needed in Mori
- works when Mori is not running (file persists until read)
- no process lifetime coupling
- debuggable (file on disk, readable by hand)
- no port conflict, no firewall question
- matches Mori's existing connector directory pattern

Why not the existing connector JSONL format: that format carries agent
session events (`session_started`, `tool_finished`, etc.). Forcing a
learning item into that shape would be a lie. A new format for a new
purpose is cleaner than stretching an existing one.

### Wire contract

```json
{
  "schemaVersion": 1,
  "transferId": "550e8400-e29b-41d4-a716-446655440000",
  "original": "The output files came back empty.",
  "answer": "출력 파일이 비어 있었습니다.",
  "kind": "translation",
  "sourceLanguage": "en",
  "targetLanguage": "ko",
  "note": "",
  "tags": ["cargo", "build"],
  "createdAt": "2026-09-03T14:30:00Z"
}
```

What is sent:

- `schemaVersion` · integer, starts at 1. Mori refuses versions it
  does not understand.
- `transferId` · opaque UUID. Lens generates it. Mori uses it for
  deduplication. Lens does not interpret it.
- `original` · the source text the reader hovered.
- `answer` · the translation or analysis.
- `kind` · `"translation"` or `"analysis"`.
- `sourceLanguage` · BCP-47 if known, empty string if not.
- `targetLanguage` · BCP-47 if known, empty string if not.
- `note` · the reader's note, if any. Empty string if none.
- `tags` · the reader's tags, if any. Empty array if none.
- `createdAt` · ISO 8601, when the item was originally saved in Lens.

What is not sent:

- Lens database row id (internal)
- review scheduling state (Mori has its own)
- transfer bookkeeping beyond `transferId`
- SQLite-specific values
- application name (privacy: a list of what programs somebody reads
  in is a different fact from what they read)
- history metadata or provenance
- caveat field, model name, footer text

### File naming

`{transferId}.json` in `~/.mori/connectors/lens/`. The transfer ID is
already a UUID, so collision is not a concern. Mori reads the directory,
processes each file, and deletes it on success (or moves it to a
`processed/` subdirectory for audit).

### Ownership

| | Lens | Mori |
|---|---|---|
| Before transfer | owns the library row | knows nothing |
| After transfer | still owns its row | owns its own copy |
| Deletion | deleting in Lens does not affect Mori | deleting in Mori does not affect Lens |

The two databases are independent. No shared primary key, no foreign
key, no sync. `transferId` is an idempotency key, not a shared
identity.

### Transfer trigger

Explicit user action only. The reader presses "Send to Mori" on a
library item. No automatic path exists:

- hover → no transfer
- save to library → no transfer
- persistent history → no transfer
- library item → user presses Send → transfer

The transfer is one-way. Lens never reads from Mori.

### Capability discovery

Mori writes a capability marker at
`~/.mori/connectors/lens/.capability.json` when it starts. The marker
advertises which schema versions Mori supports:

```json
{
  "product": "mori",
  "supportedSchemaVersions": [1],
  "capability": "lens-transfer"
}
```

Lens reads this file to determine Mori's availability and version
compatibility. Four states:

| State | Meaning | Lens behaviour |
|---|---|---|
| `not-installed` | `~/.mori/connectors/` does not exist | Send button hidden |
| `no-capability-marker` | Connectors dir exists but no `.capability.json` | Send button hidden; Mori too old or has not run |
| `version-mismatch` | Marker exists but no common schema version | Send button hidden; versions incompatible |
| `supported` | Both sides agree on a schema version | Send button visible |

If the directory exists but Mori does not process the files (Mori is
installed but not running), the files accumulate. Lens reports "sent"
(file written) and does not know or care whether Mori has read them
yet.

### Failure semantics

| Failure | Lens behaviour |
|---|---|
| Connectors directory does not exist | Send button hidden; no error |
| Capability marker missing | Send button hidden; Mori too old or not run yet |
| Schema version mismatch | Send button hidden; upgrade either side |
| Disk full | typed error, retryable |
| Permission denied | typed error, not retryable without fix |
| JSON serialization failure | typed error, not retryable (bug) |

Lens does not retry on a timer. The user can retry explicitly.

### Duplicate semantics

Mori decides. If Mori receives two files with the same `transferId`,
Mori keeps one and discards the other. Lens does not track what Mori
has already received. Lens can send the same item twice (the user
presses Send twice) and the result is Mori's problem.

### Versioning

`schemaVersion` starts at 1. Lens checks the capability marker before
showing the Send button. If Mori's `supportedSchemaVersions` does not
include Lens's version, the button stays hidden. If a later version
adds fields, Mori ignores unknown fields (tolerant reader). If a later
version removes or changes a required field, the version number
increases and older Mori refuses the file with a typed error.

### Privacy

The transfer is an explicit user action. What crosses the boundary is
previewed before it crosses: the reader sees the original, the answer,
the note, and the tags in a confirmation dialog. The reader confirms
before the file is written.

The application name is not included. A list of which programs
somebody reads in is a different fact from what they read, and the
default is to not include it.

### Local authentication

Not implemented in v1. The boundary is a file on disk, and any process
running as the same user can write to it. A local token would defend
against another process on the same machine injecting items, which is
a threat model this product does not currently address. If it arrives,
it arrives as a `schemaVersion: 2` addition.

## Consequences

- Lens writes files. Mori reads them. Neither process needs to be
  running when the other writes.
- The boundary is a directory and a JSON schema. No HTTP, no socket,
  no shared library.
- A reader who uses only Lens never notices Mori exists.
- A reader who uses both can send items from one to the other, one at
  a time, on purpose.
- The two databases stay independent. No sync, no shared identity.
- Mori needs to implement a reader for `~/.mori/connectors/lens/`.
  Until it does, files accumulate and nothing breaks.
