# Contributing to Mori Lens

## Quick start

1. Find or open an [issue](https://github.com/swarea/mori-lens/issues).
2. Branch from an up-to-date `main`: `<type>/<area>-<slug>`.
3. Commit per [commit format](docs/commit-format.md).
4. Run `scripts/check.sh` before pushing.
5. Open a pull request linking the issue with `Closes #N`.

## Branches

Branch naming:

```
<type>/<area>-<slug>
```

`<type>` is one of: `feat`, `fix`, `perf`, `refactor`, `test`, `docs`,
`chore`. Use `spike/` for throwaway investigation branches that are
never merged.

`<area>` is the code identifier: `hover`, `acquire`, `segmentation`,
`overlay`, `translation`, `ocr`, `probe`, `platform-windows`,
`platform-macos`.

Branches are short-lived, hours to a few days. A branch that cannot
merge within a few days is a sign the unit of work is too large.

## Commits

Follow [docs/commit-format.md](docs/commit-format.md). Summary:

```
<type>: <imperative lower-case summary>

- <what changed / why>
- <what changed / why>
- <what changed / why>
```

The body is a bullet list only. No prose paragraphs. No AI footer, no
`Co-Authored-By:`, no emoji.

The pull request title follows the same format.

## Merging

Pull requests merge with `--no-ff` (a merge commit, not squash). Commit
SHAs are cited in agent memory files, issue bodies, and hand-off notes;
rebase-merge replays them as new SHAs and silently breaks every
reference.

Self-merge is normal. The review step is the author re-reading the diff
against the issue plus a green verification run.

After merge, delete the branch on the remote and prune locally:

```bash
gh pr merge <n> --merge --delete-branch
git checkout main && git pull && git remote prune origin
```

## Verification

One script defines "verified":

```bash
scripts/check.sh           # fmt · clippy · tests
scripts/check.sh --full    # plus a release build of the probe
```

A green `cargo test` does not verify a platform adapter. A change to
`crates/platform-windows/` is verified by running `apps/hover-probe`
against named applications and recording the result. A pull request
that changes the adapter and shows only a green test run has not been
verified.

Never report an application as supported without a probe line for it.

## What "shipped" means

This repository is a measurement instrument before it is a product.

- A change to `crates/lens-core/` is verified by `cargo test`. It is
  pure code with no OS types.
- A change to `crates/platform-windows/` is verified by a live probe
  run against named applications, recorded in the pull request and
  transcribed into
  [`docs/compatibility/windows-app-matrix.md`](docs/compatibility/windows-app-matrix.md).
- Failure reasons are distinct: `unsupported`, `empty`, `timeout`,
  `stale`, `protected`, `element-gone`. Collapsing them into "did not
  work" destroys the data.

State which of code, test, or a live run is the basis for each claim.

## Language

Repository artifacts are English: commits, issues, pull requests, code
comments, and documentation. The product's own UI copy will be
Korean-first.

Refer to a surface by its code identifier, not a UI label.

## Direct-to-`main`

Direct commits to `main` are allowed only for:

- A non-executing documentation fix: a typo, a broken link, a stale path.
- Restoring an already-broken `main`, where a pull request would leave
  the branch broken for longer.

Everything else goes through a pull request, including work by an
autonomous agent.

## More detail

The full workflow, with reasons for every rule:
[docs/git-workflow.md](docs/git-workflow.md).

The commit message spec:
[docs/commit-format.md](docs/commit-format.md).

The architecture:
[docs/architecture.md](docs/architecture.md).

The privacy model:
[docs/privacy-model.md](docs/privacy-model.md).
