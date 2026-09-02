# Evaluation Integrity Audit

Date: 2026-09-01

## Step 1 — Metric Comparability

### Automated metrics (comparable ✓)

Both P0 and P1 were re-run with the same exact-match lemma scoring:

| Metric | P0 (exact) | P1 (exact) | Delta |
|--------|-----------|-----------|-------|
| Lemma | 40% (23/57) | 63% (36/57) avg | +23pp |
| POS | 95% (54/57) | 87% (50/57) avg | -8pp |
| Structure | 85% (64/75) | 82% (62/75) avg | -3pp |

**Note:** The original P0 evaluation used prefix matching (81% lemma).
This was corrected to exact matching (40%). Both P0 and P1 automated
metrics now use exact matching and are comparable.

### Human evaluation metrics (NOT comparable ✗)

| Metric | P0 evaluation | P1 evaluation |
|--------|--------------|--------------|
| Scoring | I (incorrect) + M (misleading) separated | M (misleading) combined |
| No-word hallucination | Classified as I | Classified as M |
| Problematic rate | 12% I + 8% M = 20% | 24% M |

**The P0 rubric was more lenient.** Entries C01, C02, E03 were "incorrect"
in P0 but "misleading" in P1. Under a consistent stricter rubric, P0's
problematic rate would be higher than 20%.

**Conclusion:** The delta "P0 20% → P1 24%" is NOT directly comparable.
The automated metrics ARE comparable. The human evaluation metrics need
re-scoring on the same rubric, which requires the P0 full output (no
longer available from a previous session).

### Correction

All future comparisons must use:
1. Same automated scoring (exact match for lemma)
2. Same human evaluation rubric (defined in Step 3)
3. Same corpus, model, temperature, profile, counterpart configuration
4. Same number of runs

## Step 2 — Corpus Expectation Audit

### Entries under review

**C01 / C02 / E03: No-word entries**

C01: "The output files came back empty." — No focused word.
Expected: lemma="", pos="", sense=""
Model fills POS and sense → hallucination.
**Audit: Expected behavior is correct. No word = no lemma/pos/sense.**

C02: "The model is up to date with the latest release." — No focused word.
Expected: lemma="", pos="", sense=""
Model fills POS and sense → hallucination.
**Audit: Expected behavior is correct.**

E03: "So quickly did the model respond that nobody noticed the latency." — No focused word.
Expected: lemma="", pos="", sense=""
Model fills POS and sense → hallucination.
**Audit: Expected behavior is correct.**

**C01 / C02: Note expectation for Reader profile**

C01 expects idiom note ("come back empty" phrasal verb).
C02 expects idiom note ("up to date" adjective idiom).

But the production prompt says: "Most sentences need no note and an empty
string is the expected answer." This suppresses note for Reader profile.

**Audit: The corpus expectation is WRONG for Reader profile.** The Reader
profile is not designed for idiom detection. The Developer profile
correctly fills notes for jargon. The corpus should not expect notes for
C01/C02 under Reader profile.

Correction: C01/C02 `expected_idiom` should be `Some(false)` for Reader,
`Some(true)` for Developer. Or we accept that Reader does not detect
idioms by design.

**D02: nearest → near**

D02: "Returns the nearest text range to a screen point." word: nearest
Expected lemma: "near"

"nearest" is the superlative form of "near". The dictionary form (lemma)
of "nearest" is indeed "near". The model returned "nearest" (unchanged).

**Audit: Expected lemma "near" is correct. The model failed to reduce
the superlative to its base form.**

**F01: vacuously → vacuously**

F01: "vacuously" word: vacuously
Expected lemma: "vacuously"

"vacuously" is already in its base form (it's an adverb derived from
"vacuous", but "vacuously" itself has no further reduction). The lemma
of an adverb is the adverb itself.

**Audit: Expected lemma "vacuously" is correct. Single-word entry,
lemma is the word itself.**

**F02: Worklynx proper noun**

F02: "The Worklynx dashboard shows one row per session." word: Worklynx
Expected lemma: "Worklynx"

"Worklynx" is a proper noun (product name). Proper nouns do not change
form. The lemma is the word itself.

**Audit: Expected lemma "Worklynx" is correct. Proper nouns are their
own lemma.**

### Summary of audit

| Entry | Issue | Verdict |
|-------|-------|---------|
| C01/C02/E03 | No-word hallucination | Expected behavior correct |
| C01/C02 | Note expectation for Reader | **Corpus expectation wrong** — Reader doesn't detect idioms |
| D02 | nearest → near | Expected correct, model failed |
| F01 | vacuously → vacuously | Expected correct |
| F02 | Worklynx → Worklynx | Expected correct |

## Step 3 — Human Evaluation Rubric (fixed)

All future M5 evaluations use this rubric:

### Per-field scoring
```
C = correct: the field is accurate and complete
P = partially correct: the field has some correct content but is incomplete or partially wrong
I = incorrect: the field is wrong
N/A = not applicable: the field should be empty (no word focused, single word with no structure, etc.)
```

### Overall scoring
```
U = useful: the explanation helps the reader understand the sentence
N = neutral: the explanation is neither helpful nor harmful
M = misleading: the explanation contains wrong information that could cause the reader to form incorrect knowledge
```

### Incorrect vs Misleading (critical distinction)

```
Incorrect (I) for a field:
  - Lemma is wrong but the reader would likely notice
  - Structure is incomplete but not confidently wrong
  - Sense is vague but not actively misleading

Misleading (M) for overall:
  - Wrong grammatical explanation stated confidently
  - Invented POS/sense for a no-word entry (hallucination)
  - Mixed-language output (e.g., Chinese characters in Korean text)
  - Structure description that is confidently wrong
  - Any error that a learner would accept as correct and remember
```

### Key rule

**No-word hallucination is always M (misleading), not I (incorrect).**
When the model invents POS/sense for an entry with no focused word, it
creates false information that the reader has no reason to doubt. This
is misleading, not merely incorrect.

## Step 4 — Misleading Regression Set

These 6 entries must pass (not M) before any prompt change can be committed:

| ID | Source | Word | Failure mode |
|----|--------|------|-------------|
| A04 | A card that the model has already answered is served from cache. | answered | Wrong lemma + mixed-language structure |
| B03 | Not only does the model translate, it also explains. | (none) | Wrong structure for inversion |
| C01 | The output files came back empty. | (none) | No-word hallucination (POS/sense) |
| C02 | The model is up to date with the latest release. | (none) | No-word hallucination (POS/sense) |
| E02 | The data, having been processed, was written to the output. | processed | Wrong lemma + wrong POS + mixed-language |
| E03 | So quickly did the model respond that nobody noticed the latency. | (none) | No-word hallucination (POS/sense) |

Subtypes:
- No-word hallucination: C01, C02, E03
- Wrong lemma: A04, E02
- Wrong structure: B03
- Mixed-language output: A04, E02
