# P13 Human Review Worksheet

**Baseline:** 2026-09-21-054306
**Corpus revision:** canary-25-v1
**Model:** qwen2.5:7b
**Reviewer:** _________________
**Date:** _________________

## Instructions

For each case below, review the source text and model output, then score using the rubric in `docs/p13-benchmark-guide.md`. Add notes for particularly good or bad outputs.

## Quick Lane (Translation Quality)

Score each translation on a 0-2 scale:
- 2 (correct): Meaning fully preserved, natural, no material issues
- 1 (partial): Meaning mostly preserved, minor issues
- 0 (wrong): Meaning lost, major omission/addition, or factually incorrect

| ID | Category | Source | Output | Meaning | Fluency | Completeness | Notes |
|----|----------|--------|--------|---------|---------|--------------|-------|
| A01 | general | The pointer rests on the sentence. | | /2 | /2 | /2 | |
| A02 | general | The card appears when the pointer settles, which takes a moment. | | /2 | /2 | /2 | |
| A03 | general | The acquirer reads the full sentence rather than a single character. | | /2 | /2 | /2 | |
| A04 | general | A card that the model has already answered is served from cache. | | /2 | /2 | /2 | |
| A05 | general | If the model is unavailable, the card says so. | | /2 | /2 | /2 | |
| B01 | grammar | The lease has expired and the model is no longer resident. | | /2 | /2 | /2 | |
| B02 | grammar | I would rather it failed loudly than passed silently. | | /2 | /2 | /2 | |
| B03 | grammar | Not only does the model translate, it also explains. | | /2 | /2 | /2 | |
| B04 | grammar | Placed on the wrong sentence, the card is worse than none. | | /2 | /2 | /2 | |
| B05 | grammar | The reader asked for explaining the sentence. | | /2 | /2 | /2 | |
| C01 | idiom | The output files came back empty. | | /2 | /2 | /2 | |
| C02 | idiom | The model is up to date with the latest release. | | /2 | /2 | /2 | |
| C03 | idiom | The build is green. | | /2 | /2 | /2 | |
| C04 | idiom | This is a no-op for existing sessions. | | /2 | /2 | /2 | |
| D01 | developer | Error: the adapter returned null for a non-optional field. | | /2 | /2 | /2 | |
| D02 | developer | Returns the nearest text range to a screen point. | | /2 | /2 | /2 | |
| D03 | developer | This change removes an assumption the code was built on. | | /2 | /2 | /2 | |
| D04 | developer | Acquisition is solved and targeting is not. | | /2 | /2 | /2 | |
| D05 | developer | The token is valid but the lease has expired. | | /2 | /2 | /2 | |
| E01 | exam | Were the model to fail, the fallback would engage immediately. | | /2 | /2 | /2 | |
| E02 | exam | The data, having been processed, was written to the output. | | /2 | /2 | /2 | |
| E03 | exam | So quickly did the model respond that nobody noticed the latency. | | /2 | /2 | /2 | |
| F01 | edge | vacuously | | /2 | /2 | /2 | |
| F02 | edge | The Worklynx dashboard shows one row per session. | | /2 | /2 | /2 | |
| F03 | edge | The timeout is 404ms, which exceeds the 200ms budget. | | /2 | /2 | /2 | |

## Deep Lane (Analysis Quality)

For each analysis, score the fields on a 0-2 scale. Check the JSON report for the actual model output.

### Word-Specific Checks (when word is present)

| ID | Word | Lemma Match | POS Match | Notes |
|----|------|-------------|-----------|-------|
| A01 | rests | ✓/✗ | ✓/✗ | Expected: rest, 동사 |
| A02 | settles | ✓/✗ | ✓/✗ | Expected: settle, 동사 |
| A04 | answered | ✓/✗ | ✓/✗ | Expected: answer, 동사 |
| A05 | unavailable | ✓/✗ | ✓/✗ | Expected: unavailable, 형용사 |
| B01 | expired | ✓/✗ | ✓/✗ | Expected: expire, 동사 |
| B02 | failed | ✓/✗ | ✓/✗ | Expected: fail, 동사 |
| B04 | Placed | ✓/✗ | ✓/✗ | Expected: place, 분사 |
| B05 | explaining | ✓/✗ | ✓/✗ | Expected: explain, 동명사 |
| C03 | green | ✓/✗ | ✓/✗ | Expected: green, 형용사 |
| D01 | adapter | ✓/✗ | ✓/✗ | Expected: adapter, 명사 |
| D02 | nearest | ✓/✗ | ✓/✗ | Expected: near, 형용사 |
| D03 | assumption | ✓/✗ | ✓/✗ | Expected: assumption, 명사 |
| D04 | targeting | ✓/✗ | ✓/✗ | Expected: targeting, 명사 |
| D05 | lease | ✓/✗ | ✓/✗ | Expected: lease, 명사 |
| E01 | fail | ✓/✗ | ✓/✗ | Expected: fail, 동사 |
| E02 | processed | ✓/✗ | ✓/✗ | Expected: process, 분사 |
| F01 | vacuously | ✓/✗ | ✓/✗ | Expected: vacuously, 부사 |
| F02 | Worklynx | ✓/✗ | ✓/✗ | Expected: Worklynx, 명사 |
| F03 | timeout | ✓/✗ | ✓/✗ | Expected: timeout, 명사 |

### Analysis Quality (all entries)

For each entry, review the JSON output and score:
- **Lemma/POS:** 0-2 (factual correctness)
- **Sense:** 0-2 (meaning grounded in context)
- **Grammar:** 0-2 (explanation correctness)
- **Structure:** 0-2 (clause analysis accuracy)
- **Usefulness:** 0-2 (helps reader understand)
- **Misleading:** ✓/✗ (contains confusing information)

| ID | Category | Lemma/POS | Sense | Grammar | Structure | Usefulness | Misleading | Notes |
|----|----------|-----------|-------|---------|-----------|------------|------------|-------|
| A01 | general | /2 | /2 | /2 | /2 | /2 | ✓/✗ | |
| A02 | general | /2 | /2 | /2 | /2 | /2 | ✓/✗ | |
| A03 | general | - | - | /2 | /2 | /2 | ✓/✗ | no-word entry |
| A04 | general | /2 | /2 | /2 | /2 | /2 | ✓/✗ | |
| A05 | general | /2 | /2 | /2 | /2 | /2 | ✓/✗ | |
| B01 | grammar | /2 | /2 | /2 | /2 | /2 | ✓/✗ | |
| B02 | grammar | /2 | /2 | /2 | /2 | /2 | ✓/✗ | |
| B03 | grammar | - | - | /2 | /2 | /2 | ✓/✗ | no-word entry |
| B04 | grammar | /2 | /2 | /2 | /2 | /2 | ✓/✗ | |
| B05 | grammar | /2 | /2 | /2 | /2 | /2 | ✓/✗ | |
| C01 | idiom | - | - | /2 | /2 | /2 | ✓/✗ | no-word entry |
| C02 | idiom | - | - | /2 | /2 | /2 | ✓/✗ | no-word entry |
| C03 | idiom | /2 | /2 | /2 | /2 | /2 | ✓/✗ | |
| C04 | idiom | - | - | /2 | /2 | /2 | ✓/✗ | no-word entry |
| D01 | developer | /2 | /2 | /2 | /2 | /2 | ✓/✗ | |
| D02 | developer | /2 | /2 | /2 | /2 | /2 | ✓/✗ | |
| D03 | developer | /2 | /2 | /2 | /2 | /2 | ✓/✗ | |
| D04 | developer | /2 | /2 | /2 | /2 | /2 | ✓/✗ | |
| D05 | developer | /2 | /2 | /2 | /2 | /2 | ✓/✗ | |
| E01 | exam | /2 | /2 | /2 | /2 | /2 | ✓/✗ | |
| E02 | exam | /2 | /2 | /2 | /2 | /2 | ✓/✗ | |
| E03 | exam | - | - | /2 | /2 | /2 | ✓/✗ | no-word entry |
| F01 | edge | /2 | /2 | /2 | /2 | /2 | ✓/✗ | |
| F02 | edge | /2 | /2 | /2 | /2 | /2 | ✓/✗ | |
| F03 | edge | /2 | /2 | /2 | /2 | /2 | ✓/✗ | |

## Summary

After completing the review, calculate:
- **Quick average:** (sum of all meaning scores) / (25 * 2)
- **Deep average:** (sum of all field scores) / (total applicable fields * 2)
- **Misleading count:** number of entries marked ✓

## Automated Baseline Results (for reference)

**Quick lane:**
- 24/25 completed (96%)
- 96% non_empty, 96% target_language_detected
- Median latency: 1538ms

**Deep lane:**
- 25/25 completed (100%)
- 100% non_empty, 100% schema_valid, 100% target_language_detected
- 96% structure_present
- 42.1% lemma_match, 84.2% pos_match (deterministic checks)
- Median latency: 7526ms

## Next Steps

1. Complete the worksheet above
2. Update the JSON report with human scores
3. Regenerate the markdown report
4. Document findings in the PR description
