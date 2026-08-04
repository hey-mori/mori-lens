# Privacy model

Written before the code that has to keep it, so it is a constraint on
the implementation rather than a description of one.

Lens reads whatever the user is reading. That reach is the product, and
it is also the reason the rules below are not adjustable defaults.

## 1. The promises

**Nothing leaves the machine by default.** Translation runs against a
local Ollama on `127.0.0.1`. A cloud provider requires an explicit
per-provider grant, an unknown endpoint counts as external, and a remote
Ollama address is a cloud provider because the text leaves the machine
([ADR-0007](decisions/ADR-0007-local-first-inference.md)).

**Nothing is recorded.** There is no screen recording, no keylogging, no
clipboard history, and no automatic transcript of what was hovered. The
translation cache, when it exists, is opt-in and holds text the user
looked at, never an image.

**No screen image is ever written to disk.** Including for debugging.
Including in a debug build. An OCR crop lives in memory for the duration
of one recognition and is dropped.

**Protected fields are refused before they are read.** `IsPassword` is
checked before any text is materialised into a string. If the check
itself fails, the field is treated as protected: unreadable means
unprovable, and unprovable is treated as secure.

**Verified, 2026-08-04.** This was the strongest promise here and the
one that had never executed. `crates/platform-windows/src/live_tests.rs`
builds a real Win32 window with two `EDIT` controls, identical except
that one carries `ES_PASSWORD`, and points the acquirer at each. The
password field returns `Protected(Declared)`, meaning the provider
affirmatively said so rather than the check falling through to the
unprovable branch.

A second check exists because the first is not enough on its own: a
build that refused *everything* would pass it and look like a working
guarantee while the product read nothing at all. The plain field must
reach a different outcome, and it does.

Run them with
`cargo test -p lens-platform-windows -- --ignored --nocapture`.

**The engine in use is visible.** The card shows what produced the
answer, so the user can always tell whether that text left the machine.

**Model output is untrusted.** Screen text is never merged into a system
prompt, the model gets no tools, and nothing it returns is executed,
opened, or followed. A page that says "ignore previous instructions" is
content to translate.

## 2. Where the pointer is allowed to look

Three tiers, per application:

| Tier | Behaviour |
|------|-----------|
| Allow | automatic hover |
| Modifier | only while a modifier key is held |
| Deny | never, including with the modifier |

Deny is the default for password managers, banking and payment
applications, and Lens itself. Lens excluding itself is not politeness;
without it the overlay reads its own translation and translates the
translation.

The first version defaults to **allow-list plus modifier elsewhere**
rather than global automatic hover. A tool that reads everything from
first launch has to earn that, and it cannot earn it before the
compatibility matrix says how well it reads anything.

## 3. Logging

The rule: **log the shape, never the content.**

Good:

```
acquire.ok source=accessibility app=a3f19c chars=87 latency_ms=14
acquire.fail source=accessibility app=a3f19c reason=unsupported
```

Forbidden in any build that a user can run:

```
acquire.ok text="the sentence the user was reading"
```

A debug build may opt into content logging behind an explicit flag that
is off by default and compiled out of release.

The probe prints no content by default. It has exactly one opt-in,
`--show-word`, which prints the **resolved word** and nothing wider:
never the sentence, never the paragraph, never the window title. It
announces itself in the startup banner for as long as it is on.

The narrow exception exists because the compatibility matrix needs a
correct-word judgement and a human cannot make one blind. The cheaper
way to get that judgement is `--expect <word>`: the operator says what
is under the pointer and the probe answers MATCH or MISMATCH without
printing anything. Prefer it. `--show-word` is for the case where the
operator is reading real prose and cannot name every word in advance.

Application identity in logs is a stable hash, not a process name, so a
log file does not enumerate what the user runs.

## 4. Permissions, asked late

| Permission | When it is requested | For what |
|------------|---------------------|----------|
| Accessibility (macOS) | first run | reading text under the pointer |
| Screen recording (macOS) | first OCR use, not before | the OCR fallback only |

Windows needs no explicit grant for UI Automation, which makes it easy
to forget that the same staging discipline applies: OCR is manual-only
in v1 on both platforms, so a user who never reaches for it never has a
capture path enabled at all.

Known limitation, stated rather than worked around: a Lens running
without elevation cannot read an application running elevated, because
of UIPI. Lens does not run elevated to fix this. Running a
screen-reading tool with administrator rights to widen its reach is the
wrong trade.

## 5. What the user can verify

Claims are worth what a user can check.

- The tray shows the current engine and whether it is local.
- A pause control stops all acquisition, and pause means the pointer is
  not sampled, not that results are hidden.
- The per-application tier list is visible and editable.
- Network calls to anything other than a granted provider are a defect,
  and the local-only case makes zero of them.

## 6. Current state

The probe today: refuses protected fields, logs no content, takes no
screen capture, makes no network call, writes no file. Those are
properties of the code as written, not aspirations.

Everything else on this page describes components that do not exist
yet. This section is updated when they do.
