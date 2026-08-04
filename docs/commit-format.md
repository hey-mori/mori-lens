# Mori Lens commit message format

Every commit in this repository follows one fixed format. It is not a
preference, it is a hard rule, the same way the verification gate is a
hard rule.

This format is inherited verbatim from
[`swarea/mori`](https://github.com/swarea/mori)'s `docs/commit-format.md`
and kept identical on purpose. The two repositories are independent
products ([ADR-0001](decisions/ADR-0001-independent-repository.md)) but they share
one maintainer and the same agents, and a history that reads the same
way in both is worth more than a format tuned per repository.

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
  - `perf` is for a change whose *point* is speed or resource use, and it
    earns its own type because `refactor` would describe it less
    accurately. A restructuring that happens to be faster is still
    `refactor`. Mori's own commit-format document omits `perf` while its
    history uses it nine times and branches on it
    (`perf/prewarm-at-startup`); `swarea/areabase` documents it with this
    rationale. Lens follows the practice, not the stale document.
  - Do not add a scope. Use `feat: ...`, never `feat(scope): ...`.
  - The summary is a lower-case, imperative-mood English phrase
    ("add …", "make …", "stop …"), no trailing period.
  - Keep it under ~70 characters.
- **Body**
  - One blank line after the subject, then a **bullet list only**.
  - Every body line is a bullet starting with `- `.
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
- Return NoTextPattern rather than an empty string when the provider
  exposes no text, so a failure can be attributed to the provider
```

## The pull request title is a commit subject

A pull request title follows this same spec: `<type>: <lower-case
imperative summary>`. It is not a headline, and it is not sentence case.

This is not a stylistic preference. `Merge pull request #N from
<branch>` is what lands in the graph, so the title is what a reader sees
in the pull request list and in every notification, and a list that
mixes `feat: read the word under the pointer` with `Read the word under
the pointer` reads as two projects. Both sibling repositories do it this
way without exception; see `swarea/mori` pull requests #37 to #42.

## Not this

A body written as prose paragraphs, even well-written prose, does
**not** conform. The body must be a bullet list.

`swarea/areabase` deliberately allows the opposite: up to three lines of
prose saying *why*, then bullets. That divergence is real and is left
alone. Lens sits next to Mori, shares its language and its release
shape, and copying one sibling exactly is worth more than splitting the
difference between two.
