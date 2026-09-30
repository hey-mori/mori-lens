# Mori Lens commit message format

Every commit in this repository follows one fixed format. It is not a
preference, it is a hard rule, the same way the verification gate is a
hard rule.

This document is self-contained. Every rule below states its own
reason, so nothing here has to be looked up somewhere else to be
followed or to be argued with.

## Shape

```
<type>: <imperative summary>
                              ← one blank line
- <what changed / why>
- <what changed / why>
- <what changed / why>
```

## Rules

- **Subject line**
  - Starts with a `type:` prefix, one of `feat`, `fix`, `perf`,
    `refactor`, `test`, `docs`, `chore`.
  - `perf` is for a change whose *point* is speed or resource use. It
    earns its own type because the alternative labels lie: `refactor`
    says the shape changed when the timing is the change, and `fix` says
    something was broken when it merely cost too much. A restructuring
    that happens to be faster is still `refactor`.
  - Do not add a scope. Use `feat: ...`, never `feat(scope): ...`.
  - The summary is a lower-case, imperative-mood English phrase
    ("add …", "make …", "stop …"), no trailing period.
  - Keep it under ~70 characters.
- **Body**
  - One blank line after the subject, then a **bullet list only**.
  - Every body line is a bullet starting with `- `. That is literal: a bullet
    that needs more words stays on one line however long it gets, because a
    wrapped continuation line is not a bullet and is rejected.
  - **Bullets are consecutive. No blank line between them.** The body is one
    list, and a blank line between bullets makes `git log` read as paragraphs,
    which is the shape the bullet rule exists to prevent. It also renders as
    several separate lists rather than one wherever a commit body is shown as
    Markdown. This was the unanimous convention long before it was written down
    here: 748 of the 765 multi-bullet commits in this repository are
    consecutive, and the 17 that are not slipped through because the validator
    skipped blank lines rather than because anybody chose that shape.
  - **No prose paragraphs.** If a point needs explanation, it is still
    one bullet: write a fuller sentence in that bullet rather than
    dropping into paragraph form.
  - English. Each bullet states what changed or why.
  - 3 to 7 bullets is typical; a trivial change may have fewer.
  - A trivial, self-explanatory change (a typo, a broken link, a
    version bump) may be **subject-only** with no body. Padding a
    one-line fix out to three bullets produces filler, which is the
    thing the bullet rule exists to prevent.
  - The bullet-only rule is an anti-slop measure, not decoration. Most
    commits here are written by an AI agent, and prose bodies drift
    into unfalsifiable summary ("improves the overall experience").
    Bullets force discrete, checkable claims.
- **Never include**
  - emoji, an AI footer, `Co-Authored-By:` lines, "Generated with"
    lines, validation logs, or "no changes" filler.
  - phase or milestone markers in the subject, such as `(M1)`,
    `(P2.5)`, `[Milestone 1]`. The branch and the issue already carry
    that, and a subject line that starts with a label reads worse a
    year later.

Every rule above is checked by `scripts/validate-commit-message.sh`, which the
commit-msg hook, the pre-push hook and `scripts/check-commit-policy.sh` all call,
and which `.githooks/tests/test-validate-commit-message.sh` covers. A rule here
that the validator does not enforce is a rule that will be broken, which is the
history of the blank-line rule above.

The `Co-Authored-By` ban is deliberate and outranks any default an
agent harness supplies. Attribution here is the commit author field;
a trailer that says a model wrote it adds no information a reader can
act on and appears on literally every commit.

## Example

```
feat: resolve the word under the pointer through ui automation

- Add UiaTextAcquirer, walking ElementFromPoint to RangeFromPoint
- Expand the range by word and read the enclosing line for context
- Report bounding rectangles in physical screen coordinates
- Return NoTextPattern rather than an empty string when the provider exposes no text, so a failure can be attributed to the provider
```

The last bullet is long and stays on one line. An earlier version of this
example wrapped it, and that version does not pass
`scripts/validate-commit-message.sh`: the rule is one bullet per line, not one
bullet per paragraph. An example that the validator rejects is worse than no
example, because it is the thing people copy.

## The pull request title is a commit subject

A pull request title follows this same spec: `<type>: <lower-case
imperative summary>`. It is not a headline, and it is not sentence case.

Two reasons, both practical:

- **The merge commit does not carry it.** What lands in the graph is
  `Merge pull request #N from <branch>`, so the title's whole audience is
  the pull request list, the notification, and the release notes. A list
  that mixes `feat: read the word under the pointer` with `Read the word
  under the pointer` reads as two projects.
- **It has to be searchable the same way commits are.** Filtering a year
  of work for every `perf:` change should not miss the ones whose author
  felt like writing a headline that day.

**Write it for the net change, not for the branch.** A branch named
`perf/…` can land as `fix:` if what it ultimately does is repair
something. The title describes what merged, and it is the last thing to
write, not the first.

**Say what changes for a reader.** `fix: make the font-size setting
reach most of the app` is a good subject. `fix: update token adoption` is
not, because it names the mechanism and leaves out the point.

## Not this

A body written as prose paragraphs, even well-written prose, does
**not** conform. The body must be a bullet list.

The reasoning that does not fit in bullets goes in the pull request,
which is where a reviewer looks for evidence, tables, and measurements.
A commit is read years later through `git blame` by someone with no
context; a pull request is read once. Writing the same argument in both
is how commit bodies grow until nobody reads them.
