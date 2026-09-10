# Git and GitHub workflow

How work moves from an idea to `main` in this repository. Commit
*message* format is a separate, stricter contract in
[`commit-format.md`](commit-format.md); this file covers branches, pull
requests, issues, CI, and releases.

Every rule here states its own reason. A rule whose only justification
is that some other repository does it the same way cannot be argued
with, cannot be applied to a case it did not anticipate, and rots the
day that repository changes.

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
7. **Merge, then delete the branch, on the remote and here.**

   ```bash
   gh pr merge <n> --merge --delete-branch
   git checkout main && git pull && git remote prune origin
   ```

   The prune is the part that gets forgotten, and it is the part
   somebody notices. `--delete-branch` removes the branch from GitHub
   and leaves this checkout's copy of it behind, so a clone that has
   done fifty of these lists fifty branches that no longer exist. It
   happened here: `git branch -r` showed fifty-five, of which one was
   real.

   The other half is a branch merged without `--delete-branch` at all,
   which then sits on the remote looking current. One did, for a day.
   `gh api repos/<owner>/<repo>/branches --jq '.[].name'` is the check
   that answers this without trusting the local copy, because the local
   copy is the thing that goes stale.

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

**Rebase-merge is the reasonable alternative and is rejected here on one
specific ground.** A linear history is genuinely easier to read, and a
project whose commits are never cited by hash should probably prefer it.
This one cites them constantly: an agent's notes, an issue body, and a
hand-off between sessions all point at work by SHA. Rebase-merge replays
those commits as new objects, so every citation silently starts pointing
at nothing. That cost is particular to how this repository is worked,
which is why the answer here can differ from the answer elsewhere
without either being wrong.

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
summary>`, describing the net change rather than the branch. See
[`commit-format.md`](commit-format.md) for why, and for what makes a
subject good rather than merely conforming.

A pull request body states what changed, why, and **how it was
verified**. That last part is not optional, because a green `cargo test`
is not evidence that a hover works against a real desktop.

The template is a checklist, not a cage. Replace its headings with ones
that fit the change, and delete the rows that do not apply. What a
reviewer needs is the evidence and the surprises: the measurement, the
thing that turned out not to be true, the alternative that was tried and
abandoned. Five headings with one sentence under each is worse than
three paragraphs that say what happened.

**Write down what went wrong on the way.** A pull request that only
narrates the successful path teaches nobody, and the wrong belief that
cost an hour is usually the most valuable thing in the diff.

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

## 6. No direct pushes to `main`

**All changes to `main` go through pull requests.** No exceptions for
documentation-only changes, typo fixes, or autonomous agent work.

The pre-push hook in `.githooks/pre-push` rejects any push where the
remote ref is `refs/heads/main`. Install it via
`scripts/setup-git-hooks.sh`, which sets `core.hooksPath`.

Autonomous agents must never push directly to `main`, including
documentation-only changes. An agent whose work nobody reviewed needs
more isolation than a human contributor, not less. The one
bootstrapping exception, recorded so it is not mistaken for the rule:
the initial commit that creates the repository skeleton has no `main`
to branch from.

`main` is protected by the pre-push hook and by the verification gate
rather than by a hard branch-protection rule, because the repository is
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

**An issue title states the problem, not the task.** "The compatibility
matrix is empty, so M1's gate is unmeasured" survives a month in a
backlog; "Fill in the matrix" does not, because it assumes the fix and
loses the reason. Naming the problem also keeps the issue honest when
the fix turns out to be something else entirely.

A `type:decision` issue is prefixed `Decide:` and phrased as a question,
so a decision is visibly not a task and nobody closes it by writing
code.

An issue title carries **no `type:` prefix**. The `type:*` label already
answers that, and a backlog where some titles are prefixed and some are
not scans worse than either convention held consistently. Commit
subjects and pull request titles do carry the prefix, because there is
no label there to carry it for them.

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
