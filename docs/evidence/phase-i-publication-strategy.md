# Phase I: Publication Strategy Analysis

**Date**: 2026-09-25  
**Purpose**: Recommend publication strategy based on audit findings

## Audited Baseline

**Audited baseline SHA**: c48f9c36b5c3a0ea8748ea1e9650adc40dc3a342  
**Total commits (as of audited baseline)**: 1,392  
**Visibility**: PRIVATE  
**Repository**: swarea/mori-lens

The merge commit containing subsequent reports necessarily postdates the audited baseline. The final RC SHA is established externally after all publication changes have landed.

## Options

### Option A: Convert Existing Repository to Public

**What it means**: Make the current private repository public as-is.

**Pros**:
- Preserves all commit history (1,392 commits as of audited baseline)
- Preserves all PRs, issues, discussions
- No migration work required
- All branches, tags, and metadata already audited
- Evidence-based development history preserved

**Cons**:
- Commit history contains historical references to CLAUDE.md (now replaced)
- Some commit messages contain internal context (but no secrets)
- PR bodies have been sanitized (41 PRs for AI attribution, 2 PRs for PII/internal data)
- Cannot undo git history (what's committed stays committed)

**Evidence from Phases F-H**:
- **Phase F (Security Scan)**: PASS - No secrets in current tree or full git history
- **Phase G (Metadata Sanitation)**: PASS - 41 PRs sanitized for AI attribution, 2 PRs sanitized for PII/internal data
- **Phase H (Actions/Artifacts)**: PASS - All merged branches deleted, no releases, no tags, repository metadata clean

**Risk Assessment**: LOW
- No secrets detected in git history (gitleaks scan)
- All PR metadata sanitized
- No internal domains or sensitive paths in code
- Author metadata retained by owner decision
- All references to CLAUDE.md now point to CONTRIBUTING.md

### Option B: Create New Clean Repository

**What it means**: Create a new repository, cherry-pick or copy only the current code state, start with fresh history.

**Pros**:
- Completely clean slate
- No historical references to worry about
- Can curate commit messages from scratch

**Cons**:
- Loses 1,392 commits of development history (as of audited baseline)
- Loses all PR discussions, reviews, and context
- Loses issue tracking history
- Requires significant manual work to reconstruct
- Appears as if project started recently (no provenance)
- Cannot preserve the evidence-based development approach that defines this project
- Breaks all external references (documentation, benchmarks, ADRs reference commit history)

**Evidence Required**:
- Would need to re-run all audits on the new repository
- Would lose Phase F-H evidence (audits were on the original repo)
- Would need to recreate all documentation linking to commits

**Risk Assessment**: MEDIUM
- High chance of losing important context
- Labor-intensive with no security benefit (Phase F already proved no secrets)
- Destroys provenance and evidence trail

## Recommendation

**Option A: Convert Existing Repository to Public**

### Rationale

1. **Security is proven, not assumed**
   - Phase F scanned the full git history with gitleaks: 0 leaks detected
   - No secrets have ever been committed
   - No internal domains or sensitive paths in code
   - The repository is safe to make public as-is

2. **Metadata is sanitized**
   - Phase G cleaned 41 PR bodies for AI attribution
   - Phase G sanitized 2 PR bodies for PII/internal data (PR #181, PR #200)
   - No attribution in issues or comments
   - All references to AI tools removed from public-facing content

3. **Infrastructure is clean**
   - Phase H audited all branches, tags, releases, artifacts
   - Only main branch remains (all merged branches deleted)
   - No releases, no Pages, no Wiki
   - Repository metadata is public-facing and accurate

4. **History is the product**
   - This project's defining characteristic is evidence-based development
   - 1,392 commits document the measurement-first approach (as of audited baseline)
   - ADRs, benchmarks, and compatibility matrices reference commit history
   - Losing history would destroy the project's provenance

5. **Author metadata decision resolved**
   - Owner decision: Existing Git author metadata will be retained for history preservation
   - No author-history rewrite will be performed
   - Evidence documents redacted to remove PII
   - Git author emails will be publicly visible (owner accepted)

6. **No security benefit to Option B**
   - Phase F proved no secrets exist in history
   - Creating a new repo would not make it "more secure"
   - Would only lose context and provenance

### Execution Plan for Option A

1. **Merge evidence reconciliation PR**
   - Update Phase F, G, H evidence documents
   - Update final publication report
   - Ensure all gates pass

2. **Verify all gates pass on main**
   - `scripts/check.sh --full`
   - `scripts/check_public_readiness.py`
   - Re-run gitleaks to confirm
   - Re-run PII/internal-data scan

3. **Make repository public**
   - Use GitHub API or web UI to change visibility
   - No tag, no release yet (owner decision)

4. **Optional: Create v0.1.0 tag**
   - Only if owner decides to release
   - Would use `git tag -s v0.1.0 -m "v0.1.0: Initial public release"`

5. **Optional: Create GitHub Release**
   - Only if owner decides to distribute binaries
   - Would include signed installers (NSIS, MSIX)
   - Would include SHA256SUMS.txt with detached signature

## Conclusion

**PUBLICATION STRATEGY = RECOMMENDED (Option A), NOT EXECUTED**

The repository is ready for Option A publication. All audits pass. The recommendation is to convert the existing repository to public, preserving the full commit history and evidence trail.

The decision to execute (make public, tag, release) remains with the owner.

## Owner Decisions

**EVIDENCE DOCUMENT PII = REDACTED**  
**GIT AUTHOR EMAILS = RETAINED**  
**GIT HISTORY REWRITE = NO**  
**LICENSE GRANT = OWNER DECISION PENDING**  
**PUBLICATION EXECUTION = PENDING**

## Repository Status

```
PUBLIC-FACING TREE = PASS
PUBLIC-READINESS AUTOMATED GATE = PASS
CURRENT TREE SECRET/PII SCAN = PASS
FULL GIT HISTORY SCAN = PASS
GITHUB PR/ISSUE METADATA = SANITIZED
ACTIONS/ARTIFACTS/BRANCHES = AUDITED
THIRD-PARTY COMPLIANCE = PASS
ASSET PROVENANCE = PASS
LICENCE PREPARATION = COMPLETE
LICENCE GRANT = OWNER DECISION PENDING
PUBLICATION STRATEGY = RECOMMENDED (Option A), NOT EXECUTED
REPOSITORY VISIBILITY = PRIVATE
```
