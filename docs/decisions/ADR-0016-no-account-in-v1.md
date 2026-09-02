# ADR-0016 · No account in v1

**Status** · Accepted, 2026-09-03 (owner decision 2026-08-13)

## Context

`docs/roadmap.md` excludes accounts and cloud sync and says adding one is
a decision, not a task. PR #158 refused accounts on the grounds that an
account identifies a person to a server and Lens reaches none. Sound, and
too broad: it treated seven different things as one.

Seven different things need seven different answers, and only one of them
is "no, never."

| Thing | Needs a Lens account | Reaches a server |
|---|---|---|
| Accountless local mode | no | no |
| Local profile or workspace | no | no |
| Provider credential (BYOK) | no | yes, when used |
| Mori identity | no, it is Mori's | no |
| Licence or subscription | not necessarily | not necessarily |
| Multi-device sync | yes | yes |
| Team | yes | yes |

## Decision

**No Lens account in v1, and no scaffolding for one.**

- no login UI, and no placeholder Account page
- one local profile in v1
- every local feature fully usable without an account
- Mori is a connected application, not a Lens identity
- BYOK credentials need no Lens account and live in OS credential storage
- an optional account may arrive only with a real server job: first-party
  sync, a web Library, or a subscription
- local-only mode survives the introduction of an account
- identity, billing, sync and inference transport stay separate concerns
- teams remain excluded
- the engine never learns who the user is

### What triggers each row

| Thing | Revisit when |
|---|---|
| Accountless local mode | never. This is v1 |
| Local profile or workspace | a reader names a second configuration they want on the same machine |
| Provider credential (BYOK) | already supported. The key lives in OS credential storage |
| Mori identity | never from Lens. It is Mori's concern across a process boundary |
| Licence or subscription | a paid service is introduced |
| Multi-device sync | a reader with two machines has tried a synced folder and can say why it fell short |
| Team | shared collaboration is wanted by a reader who is not the builder |

### Why not scaffolding

An auth subsystem built before any of its triggers fire is a hypothesis
shaped as code. It adds surface to the privacy model, fields to the
settings schema, and an identity flow to the engine boundary, all for a
reader who does not exist yet. The triggers above are cheap to check and
expensive to predict wrong.

### What an account would not change

ADR-0007 is about where the text goes. It is superseded when hovered text
leaves the machine, and by nothing else. An account for billing does not
change where inference runs. An account for sync does not change it either,
because the text being synced was already local. What an account changes is
the privacy model, which gains an identity flow it does not have today, and
that is its own amendment to that document.

### The engine boundary

The engine is `lens-core`. No module in it, or in `lens-settings`, or in
`lens-storage`, carries any concept of user identity: no username, email,
account, auth token, user id, or session id. This is verified by the
absence of any such field in the public types of those crates and by the
absence of any Tauri command that passes identity to lens-core. A service
layer, if one arrives, attaches above the engine rather than through it.

## Consequences

- The privacy model stays simple. No identity flow means no identity
  surface to defend, no credential storage to audit, and no account
  deletion flow to honour.
- A later account is a new ADR, not a flag added to existing code. The
  clean separation is the point.
- A reader who wants sync between machines has no path today, and the
  honest answer is "not yet" rather than a workaround that pretends
  otherwise.
- The trigger table is the check. When none of them fire, the decision
  stands without needing to be re-argued.
