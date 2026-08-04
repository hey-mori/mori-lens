# Git and GitHub workflow

How work moves from an idea to `main` in this repository. Commit
*message* format is a separate, stricter contract in
[`commit-format.md`](commit-format.md); this file covers branches, pull
requests, issues, CI, and releases.

This workflow matches [`swarea/mori`](https://github.com/swarea/mori)'s
deliberately. Where Lens differs from Mori, the difference is called out
in place rather than left for a reader to notice.

## 1. The model: GitHub Flow, adapted

`main` is always releasable. Work happens on short-lived branches that
merge back through a pull request. There is no `develop`, no
`release/*`, no `hotfix/*`. Git Flow's release-train shape solves a
problem Lens does not have: there are no versioned boxed releases and no
parallel maintained majors.

The adaptation is deliberate and documented rather than left implicit.
Lens is built by one maintainer plus AI agents, and a rule that pretends
otherwise would be broken daily and would make the history *less*
honest, not more. See §6.

## 2. Branch naming

```
<type>/<area>-<slug>
```

`<type>` matches the commit-message type vocabulary, so a branch and its
commits agree: `feat`, `fix`, `perf`, `refactor`, `test`, `docs`,
`chore`. Add `spike/` for throwaway investigation branches that are
never merged.

```
feat/hover-uia-word-resolution
fix/hover-stale-generation-render
perf/acquire-wake-once-per-window
refactor/segmentation-boundary-split
docs/decisions-accessibility-first
chore/workspace-and-ci-setup
```

`<area>` is the code identifier for the surface, not a product word:
`hover`, `acquire`, `segmentation`, `overlay`, `translation`, `ocr`,
`probe`, `platform-windows`, `platform-macos`.

Branches are short-lived, hours to a few days, and deleted after merge.
A branch that cannot merge within a few days is a sign the unit of work
is too large and should be split.

## 3. The cycle

1. **Issue first** for anything non-trivial. It records the *why* and
   the evidence (`file:line`), so the pull request can stay about the
   change.
2. **Branch** from an up-to-date `main`.
3. **Commit** per [`commit-format.md`](commit-format.md). Small,
   meaningful commits: the merge preserves them (§4), so they are the
   durable record.
4. **Validate locally** before pushing: `scripts/check.sh`.
5. **Pull request** using the template. Link the issue with `Closes #N`
   so the issue closes on merge and the trail is traceable both
   directions.
6. **CI green** before merge, when CI is enabled (§9).
7. **Merge, then delete the branch.**

## 4. Merge policy: merge commit, not squash

Merge with `--no-ff` (GitHub's "Create a merge commit"), in this order
of reasons:

1. **Commit SHAs are long-lived identifiers here.** Agent memory files,
   issue bodies, and hand-off notes cite them to point at a specific
   change. Rebase-merge replays branch commits as *new* SHAs, which
   silently breaks every one of those references, including on another
   machine picking the work up later.
2. **Sessions hand work to each other by commit.** Preserving the exact
   SHA a branch produced is what makes that hand-off reliable.
3. **The pull request boundary stays in the graph**, so it is visible
   which commits were integrated as one reviewed unit, and one merge
   commit reverts the whole unit.
4. **Squashing would discard the commit discipline we pay for.**
   `commit-format.md` already forces every commit to be a well-formed
   statement of one change; collapsing a reviewed series into one blob
   throws that away.

**The organisation is split on this, and the split is deliberate.**
`swarea/areabase` merges with rebase for a linear history, and says so
in its `CONTRIBUTING.md`. Lens follows Mori instead, because reason 1
above is specific to how this pair of repositories is worked: sessions
hand each other commit SHAs, and rebase-merge invalidates every one of
them. Areabase does not work that way. Two answers to one question is
fine when both are written down; an unwritten answer is not.

What this policy does **not** claim: merge commits are not a concurrency
control. When `main` moves under a branch, both rebase and merge have to
integrate and both can conflict. Parallel work is isolated by
short-lived branches and pull requests instead of direct pushes to
`main` (§6), and that, not the merge strategy, is what makes concurrent
sessions safe.

The cost is a busier graph. That is accepted.

Never force-push a branch that others, or another machine, may have
pulled. Never rewrite `main`.

## 5. Pull requests

**The title is a commit subject**: `<type>: <lower-case imperative
summary>`, per [`commit-format.md`](commit-format.md). Not a headline,
not sentence case. Both sibling repositories hold this without
exception.

A pull request body states what changed, why, and **how it was
verified**. That last part is not optional, because a green `cargo test`
is not evidence that a hover works against a real desktop.

The template is a checklist, not a cage. Mori's own pull requests use
their own headings and narrate the finding, keeping only a verification
section that names the gate and the numbers. Follow that: what a
reviewer needs is the evidence and the surprises, not five empty
headings.

For this repository specifically: a change to the platform acquisition
path is only verified by a **probe run against named applications**,
recorded in the pull request and transcribed into
[`compatibility/windows-app-matrix.md`](compatibility/windows-app-matrix.md).
A unit test over a fake acquirer proves the state machine, not the
adapter. Both are required; neither substitutes for the other.

Pull requests are small enough to read in one sitting. If one touches
more than a couple of areas, it is usually two pull requests.

Self-merge is normal here, and is not a substitute for review: the
review step is the author re-reading the diff against the issue plus a
green verification run.

## 6. The direct-to-`main` exception (narrow)

Direct commits to `main` are allowed for exactly two cases:

- A non-executing documentation fix: a typo, a broken link, a stale
  path.
- Restoring an already-broken `main`, where opening a pull request would
  leave the branch broken for longer.

Everything else goes through a pull request, **including work by an
autonomous agent**. An agent whose work nobody reviewed needs more
isolation than a human contributor, not less.

The one bootstrapping exception, recorded so it is not mistaken for the
rule: the initial commit that creates the repository skeleton has no
`main` to branch from.

`main` is protected by convention and by the verification gate rather
than by a hard branch-protection rule, because the repository is
private on a plan where protection rules are not available. That is a
platform limitation, not a preference: when the constraint lifts, turn
protection on.

## 7. Language

Repository artifacts are written in **English**: commit messages, branch
names, issue titles and bodies, labels, milestones, pull requests, code
comments, and these docs. The product's own UI copy will be
Korean-first, and that split is deliberate: the app speaks to its user
in Korean, the repository speaks to whoever maintains it.

**Refer to a surface by its code identifier, not a UI label.** An issue
titled "hover card appears on the wrong monitor" matches the code; one
titled "번역 카드가 …" matches neither the code nor a reader.

**Identifiers are ASCII; prose follows the writing style.** The em dash
is banned in prose (see [`CLAUDE.md`](../CLAUDE.md)) and `·` is offered
as a replacement, but that rule does not govern *identifiers*. A label
name, a milestone title, and a branch name are typed into a shell,
pasted into a URL, and matched by scripts, so they use plain ASCII with
no decorative separator:

```
M1 Hover probe           not  M1 · Hover probe
area:acquire             not  area:텍스트추출
fix/hover-stale-render   not  fix/호버-stale
```

The description field is prose, and follows the prose rule.

## 8. Issues

**An issue title states the problem, not the task.** "Empty states are
split across 3 primitives" reads better in a backlog a month later than
"Fix empty states", because it says what is wrong rather than assuming
the fix. A `type:decision` issue is prefixed `Decide:` and phrased as a
question, so a decision is visibly not a task. Both conventions come
from `swarea/mori`'s backlog.

An issue title carries **no `type:` prefix**, even though Mori's newest
issues have drifted into using one (`perf: pre-warm too late to help the
first request`). The `type:*` label already says which kind it is, and a
backlog where some titles are prefixed and some are not sorts and scans
worse than either convention held consistently. Commit subjects and pull
request titles do carry the prefix, because there is no label there.

Issues carry labels along three axes so the backlog can be sliced by
surface, kind, and urgency:

- `area:*` · which surface, by code identifier: `hover`, `acquire`,
  `segmentation`, `language`, `translation`, `overlay`, `ocr`,
  `platform-windows`, `platform-macos`, `probe`, `storage`
- `type:*` · `bug`, `feature`, `compatibility`, `decision`, `docs`,
  `perf`
- `priority:*` · `high`, `medium`, `low`

`type:decision` marks an issue blocked on a product judgement rather
than on engineering, so the backlog never hides a question behind a
task. `type:compatibility` is specific to this repository: an
application that returns nothing through the accessibility tree is a
recorded fact about that application, not a bug in Lens, and filing it
as a bug would corrupt the defect count.

Milestones map to the roadmap (M0 to M8, see
[`roadmap.md`](roadmap.md)), so an issue's milestone answers "which
stage does this belong to".

## 9. Verification

One script defines "verified", and both a local run and CI call it:

```
scripts/check.sh          fmt · clippy · tests
scripts/check.sh --full   the above, plus a release build of the probe
```

CI must never re-list these commands in YAML. If it did, the local rule
and the CI rule would drift, and the pull request checklist would start
meaning two things.

**The remote CI is manual-only for now, matching Mori, and for the same
reason.** The repository is private and Actions on a private repository
is metered; a permanently failing check is worse than an honestly
disabled one, because real failures hide inside the noise and everyone
learns to ignore the signal. The automatic triggers are commented out in
[`ci.yml`](../.github/workflows/ci.yml), not deleted, and restoring them
is a one-line change once billing is configured or the repository is
public.

So today the real gate is `scripts/check.sh`, run locally and recorded
in the pull request under "How it was verified".

Stating a rule the tooling cannot enforce would make this document
decorative. This section says what is actually true.

## 10. Releases

Tag a release when a milestone completes: `v0.1.0` for M1, and so on,
with notes summarizing the merged pull requests. The tag is what makes
the roadmap's progress legible from outside the repository.

No release is tagged for a milestone whose compatibility matrix is
still empty. The matrix is the evidence the milestone happened.
