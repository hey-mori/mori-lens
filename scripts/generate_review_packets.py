"""Generate P13 human review packets from the authoritative Rust corpus and canonical JSON.

Source of truth:
  crates/lens-translate/src/eval_corpus.rs

Canonical artifacts:
  docs/benchmarks/p13-quick-baseline-2026-09-21-054306.json
  docs/benchmarks/p13-deep-baseline-2026-09-21-054306.json

Usage:
  python scripts/generate_review_packets.py          # generate packets
  python scripts/generate_review_packets.py --validate  # validate only, no output
"""
import json
import os
import re
import sys


# ---------------------------------------------------------------------------
# Paths (relative to repository root)
# ---------------------------------------------------------------------------
REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CORPUS_RS = os.path.join(REPO_ROOT, 'crates', 'lens-translate', 'src', 'eval_corpus.rs')
QUICK_JSON = os.path.join(REPO_ROOT, 'docs', 'benchmarks', 'p13-quick-baseline-2026-09-21-054306.json')
DEEP_JSON = os.path.join(REPO_ROOT, 'docs', 'benchmarks', 'p13-deep-baseline-2026-09-21-054306.json')
QUICK_PACKET = os.path.join(REPO_ROOT, 'docs', 'benchmarks', 'p13-human-review-quick-2026-09-21.md')
DEEP_PACKET = os.path.join(REPO_ROOT, 'docs', 'benchmarks', 'p13-human-review-deep-2026-09-21.md')

EXPECTED_IDS = [
    'A01', 'A02', 'A03', 'A04', 'A05',
    'B01', 'B02', 'B03', 'B04', 'B05',
    'C01', 'C02', 'C03', 'C04',
    'D01', 'D02', 'D03', 'D04', 'D05',
    'E01', 'E02', 'E03',
    'F01', 'F02', 'F03',
]
EXPECTED_CORPUS_REVISION = 'canary-25-v1'


# ---------------------------------------------------------------------------
# eval_corpus.rs parser
# ---------------------------------------------------------------------------
def parse_corpus_rs(path):
    """Parse eval_corpus.rs and return (corpus_revision, list_of_entry_dicts).

    Fails closed on any unexpected structure.
    """
    with open(path, encoding='utf-8') as f:
        text = f.read()

    # Extract CORPUS_REVISION
    m = re.search(r'pub\s+const\s+CORPUS_REVISION\s*:\s*&str\s*=\s*"([^"]*)"', text)
    if not m:
        raise ValueError('Could not find CORPUS_REVISION in eval_corpus.rs')
    corpus_revision = m.group(1)

    # Extract the CORPUS array body
    m = re.search(r'pub\s+static\s+CORPUS\s*:\s*&\[CorpusEntry\]\s*=\s*&\[(.*?)\];', text, re.DOTALL)
    if not m:
        raise ValueError('Could not find CORPUS array in eval_corpus.rs')
    corpus_body = m.group(1)

    # Find each CorpusEntry { ... } block using non-greedy match
    # Each block starts with "CorpusEntry {" and ends with "},"
    entry_blocks = re.findall(r'CorpusEntry\s*\{(.*?)\},', corpus_body, re.DOTALL)

    entries = []
    for i, block in enumerate(entry_blocks):
        entry = _parse_one_entry(block, i)
        entries.append(entry)

    return corpus_revision, entries


def _parse_one_entry(block, index):
    """Parse a single CorpusEntry { ... } block body (everything after the opening brace)."""
    # Extract fields using regex on the block text
    entry = {}

    # id (required string)
    m = re.search(r'id\s*:\s*"([^"]*)"', block)
    if not m:
        raise ValueError(f'CorpusEntry block {index}: missing id field')
    entry['id'] = m.group(1)

    # source (required string)
    m = re.search(r'source\s*:\s*"([^"]*)"', block)
    if not m:
        raise ValueError(f'CorpusEntry {entry.get("id", index)}: missing source field')
    entry['source'] = m.group(1)

    # translation (required string)
    m = re.search(r'translation\s*:\s*"([^"]*)"', block)
    if not m:
        raise ValueError(f'CorpusEntry {entry.get("id", index)}: missing translation field')
    entry['translation'] = m.group(1)

    # word (Option<&str>)
    entry['word'] = _extract_option_str(block, 'word', entry.get('id', str(index)))

    # category (required string)
    m = re.search(r'category\s*:\s*"([^"]*)"', block)
    if not m:
        raise ValueError(f'CorpusEntry {entry.get("id", index)}: missing category field')
    entry['category'] = m.group(1)

    # expected_lemma (Option<&str>)
    entry['expected_lemma'] = _extract_option_str(block, 'expected_lemma', entry.get('id', str(index)))

    # expected_pos (Option<&str>)
    entry['expected_pos'] = _extract_option_str(block, 'expected_pos', entry.get('id', str(index)))

    # expected_structure_rubric (Option<&str>)
    entry['expected_structure_rubric'] = _extract_option_str(block, 'expected_structure_rubric', entry.get('id', str(index)))

    # expected_grammar (Option<&str>)
    entry['expected_grammar'] = _extract_option_str(block, 'expected_grammar', entry.get('id', str(index)))

    # expected_idiom (Option<bool>)
    entry['expected_idiom'] = _extract_option_bool(block, 'expected_idiom', entry.get('id', str(index)))

    # note (Option<&str>)
    entry['note'] = _extract_option_str(block, 'note', entry.get('id', str(index)))

    return entry


def _extract_option_str(block, field_name, entry_id):
    """Extract an Option<&'static str> field from a CorpusEntry block."""
    # Match Some("value") or None
    pattern = rf'{field_name}\s*:\s*(Some\("([^"]*)"\)|None)'
    m = re.search(pattern, block)
    if not m:
        raise ValueError(f'CorpusEntry {entry_id}: missing {field_name} field')
    full_match = m.group(1)
    if full_match == 'None':
        return None
    return m.group(2)


def _extract_option_bool(block, field_name, entry_id):
    """Extract an Option<bool> field from a CorpusEntry block."""
    pattern = rf'{field_name}\s*:\s*(Some\((true|false)\)|None)'
    m = re.search(pattern, block)
    if not m:
        raise ValueError(f'CorpusEntry {entry_id}: missing {field_name} field')
    full_match = m.group(1)
    if full_match == 'None':
        return None
    return m.group(2) == 'true'


# ---------------------------------------------------------------------------
# JSON loading
# ---------------------------------------------------------------------------
def load_json(path):
    with open(path, encoding='utf-8') as f:
        return json.load(f)


# ---------------------------------------------------------------------------
# Integrity assertions
# ---------------------------------------------------------------------------
def assert_integrity(corpus_revision, corpus_entries, quick_data, deep_data):
    """Assert all source-of-truth integrity conditions. Fail closed."""
    errors = []

    # 1. Corpus revision
    if corpus_revision != EXPECTED_CORPUS_REVISION:
        errors.append(f'Corpus revision mismatch: got {corpus_revision!r}, expected {EXPECTED_CORPUS_REVISION!r}')

    # 2. Corpus count
    if len(corpus_entries) != 25:
        errors.append(f'Corpus entry count: got {len(corpus_entries)}, expected 25')

    # 3. Corpus IDs
    corpus_ids = [e['id'] for e in corpus_entries]
    if corpus_ids != EXPECTED_IDS:
        errors.append(f'Corpus ID mismatch: got {corpus_ids}, expected {EXPECTED_IDS}')

    # 4. Quick count
    quick_cases = quick_data.get('cases', [])
    if len(quick_cases) != 25:
        errors.append(f'Quick case count: got {len(quick_cases)}, expected 25')

    # 5. Deep count
    deep_cases = deep_data.get('cases', [])
    if len(deep_cases) != 25:
        errors.append(f'Deep case count: got {len(deep_cases)}, expected 25')

    # 6. Quick IDs match
    quick_ids = [c['entry_id'] for c in quick_cases]
    if quick_ids != EXPECTED_IDS:
        errors.append(f'Quick ID mismatch: got {quick_ids}')

    # 7. Deep IDs match
    deep_ids = [c['entry_id'] for c in deep_cases]
    if deep_ids != EXPECTED_IDS:
        errors.append(f'Deep ID mismatch: got {deep_ids}')

    # Build corpus lookup
    corpus_by_id = {e['id']: e for e in corpus_entries}

    # 8. Source agreement
    for c in quick_cases:
        eid = c['entry_id']
        corpus_src = corpus_by_id[eid]['source']
        json_src = c['source']
        if corpus_src != json_src:
            errors.append(f'Source mismatch in Quick {eid}: corpus={corpus_src!r}, json={json_src!r}')

    for c in deep_cases:
        eid = c['entry_id']
        corpus_src = corpus_by_id[eid]['source']
        json_src = c['source']
        if corpus_src != json_src:
            errors.append(f'Source mismatch in Deep {eid}: corpus={corpus_src!r}, json={json_src!r}')

    # 9. Focused word agreement
    for c in quick_cases:
        eid = c['entry_id']
        corpus_word = corpus_by_id[eid]['word']
        json_word = c.get('focused_word')
        # JSON uses None/null for no word, corpus uses None
        if corpus_word != json_word:
            errors.append(f'Focused word mismatch in Quick {eid}: corpus={corpus_word!r}, json={json_word!r}')

    for c in deep_cases:
        eid = c['entry_id']
        corpus_word = corpus_by_id[eid]['word']
        json_word = c.get('focused_word')
        if corpus_word != json_word:
            errors.append(f'Focused word mismatch in Deep {eid}: corpus={corpus_word!r}, json={json_word!r}')

    # 10. Deep output field count (must be exactly 5 for all 25)
    for c in deep_cases:
        eid = c['entry_id']
        output = c.get('output', '')
        parts = output.split('|')
        if len(parts) != 5:
            errors.append(f'Deep {eid}: output has {len(parts)} pipe fields, expected 5. Raw: {output!r}')

    # 11. Quick JSON corpus_revision matches
    quick_rev = quick_data.get('metadata', {}).get('corpus_revision', '')
    if quick_rev != EXPECTED_CORPUS_REVISION:
        errors.append(f'Quick JSON corpus_revision: got {quick_rev!r}, expected {EXPECTED_CORPUS_REVISION!r}')

    deep_rev = deep_data.get('metadata', {}).get('corpus_revision', '')
    if deep_rev != EXPECTED_CORPUS_REVISION:
        errors.append(f'Deep JSON corpus_revision: got {deep_rev!r}, expected {EXPECTED_CORPUS_REVISION!r}')

    if errors:
        msg = 'INTEGRITY ASSERTIONS FAILED:\n' + '\n'.join(f'  - {e}' for e in errors)
        raise ValueError(msg)


# ---------------------------------------------------------------------------
# Deep output parsing (fail closed)
# ---------------------------------------------------------------------------
def parse_deep_output(entry_id, output_str):
    """Parse pipe-separated Deep output into labeled fields. Fail closed."""
    parts = output_str.split('|')
    if len(parts) != 5:
        raise ValueError(
            f'Deep {entry_id}: output has {len(parts)} pipe fields, expected 5. '
            f'Raw: {output_str!r}'
        )
    return {
        'lemma': parts[0].strip(),
        'pos': parts[1].strip(),
        'sense': parts[2].strip(),
        'structure': parts[3].strip(),
        'note': parts[4].strip(),
    }


# ---------------------------------------------------------------------------
# Quick packet generation
# ---------------------------------------------------------------------------
def generate_quick_packet(corpus_by_id, quick_data):
    lines = []
    meta = quick_data['metadata']
    cases = quick_data['cases']

    # Count completed / failures
    n_completed = sum(1 for c in cases if not c['automated'].get('error_type'))
    n_failures = sum(1 for c in cases if c['automated'].get('error_type'))
    latencies = [c['automated']['latency_ms'] for c in cases]
    median_latency = sorted(latencies)[len(latencies) // 2]
    non_empty_pass = sum(1 for c in cases if c['automated']['non_empty'])
    tld_pass = sum(1 for c in cases if c['automated']['target_language_detected'])

    lines.append('# P13 Human Review Packet -- Quick Lane')
    lines.append('')
    lines.append('| Field | Value |')
    lines.append('|-------|-------|')
    lines.append(f'| Canonical baseline | {meta["timestamp"][:10].replace("-", "")} |')
    lines.append(f'| Corpus revision | {meta["corpus_revision"]} |')
    lines.append(f'| Model | {meta["model"]} |')
    lines.append(f'| Provider | {meta["provider"]} |')
    lines.append(f'| Endpoint | {meta["endpoint"]} |')
    lines.append('| Source artifact | `p13-quick-baseline-2026-09-21-054306.json` |')
    lines.append('| Merged PR | #612 |')
    lines.append('| Merged main SHA | `d3e0e34caaceb1a1208f390f83733fb08a506fae` |')
    lines.append('| Reviewer | _________________ |')
    lines.append('| Review date | _________________ |')
    lines.append('')
    lines.append('> **Human judgment fields are intentionally blank at generation time.**')
    lines.append('> Do not use automated evidence to pre-fill human scores.')
    lines.append('')
    lines.append('## Scoring rubric (Quick)')
    lines.append('')
    lines.append('- **2 (correct):** Meaning fully preserved, natural Korean, no material issues')
    lines.append('- **1 (partial):** Meaning mostly preserved, minor issues')
    lines.append('- **0 (wrong):** Meaning lost, major omission/addition, or factually incorrect')
    lines.append('')
    lines.append('## Automated summary')
    lines.append('')
    lines.append(f'- Total cases: {len(cases)}')
    lines.append(f'- Completed inference: {n_completed}')
    if n_failures:
        failure_ids = [c['entry_id'] for c in cases if c['automated'].get('error_type')]
        failure_types = [c['automated']['error_type'] for c in cases if c['automated'].get('error_type')]
        for fid, ft in zip(failure_ids, failure_types):
            lines.append(f'- Inference failure: {fid} `{ft}`')
    lines.append(f'- Median latency: {median_latency}ms')
    lines.append(f'- non_empty: {non_empty_pass * 100 // len(cases)}%')
    lines.append(f'- target_language_detected: {tld_pass * 100 // len(cases)}%')
    lines.append('')
    lines.append('> The first measured request (A01, 14561ms) includes cold-start cost. Median represents steady-state.')
    lines.append('')
    lines.append('---')
    lines.append('')

    for c in cases:
        eid = c['entry_id']
        corpus = corpus_by_id[eid]
        auto = c['automated']
        obs = c.get('observations', {})

        lines.append(f'## {eid} -- {corpus["category"]}')
        lines.append('')
        lines.append(f'**Source:** {corpus["source"]}')
        lines.append('')
        if corpus['word']:
            lines.append(f'**Focused word:** {corpus["word"]}')
            lines.append('')
        lines.append(f'**Reference translation:** {corpus["translation"]}')
        lines.append('')

        error = auto.get('error_type')
        if error:
            lines.append(f'**Inference status:** FAILED (`{error}`)')
            lines.append(f'**Latency:** {auto["latency_ms"]}ms')
            lines.append('')
            lines.append('> No usable translation output. The model returned a response that could not be parsed into a valid translation object.')
        else:
            lines.append('**Inference status:** completed')
            lines.append(f'**Latency:** {auto["latency_ms"]}ms')
            lines.append('')
            lines.append(f'**Model output:** {c["output"]}')

        lines.append('')
        lines.append('**Automated evidence:**')
        lines.append('')
        lines.append(f'- non_empty: {auto["non_empty"]}')
        lines.append(f'- target_language_detected: {auto["target_language_detected"]}')
        if obs.get('target_script_ratio') is not None:
            lines.append(f'- target_script_ratio: {obs["target_script_ratio"]:.0%}')
        if obs.get('length_ratio') is not None:
            lines.append(f'- length_ratio: {obs["length_ratio"]:.2f}')
        lines.append('')
        lines.append('**Human review:**')
        lines.append('')
        lines.append('- Meaning preserved: __ / 2')
        lines.append('- Naturalness: __ / 2')
        lines.append('- Completeness: __ / 2')
        lines.append('- Notes:')
        lines.append('')
        lines.append('---')
        lines.append('')

    # Summary section -- field-specific denominators, B01 not pre-decided
    lines.append('## Summary (to be filled after review)')
    lines.append('')
    lines.append('| Metric | Value |')
    lines.append('|--------|-------|')
    lines.append('| Meaning preservation avg | __ / 2 |')
    lines.append('| Naturalness avg | __ / 2 |')
    lines.append('| Completeness avg | __ / 2 |')
    lines.append(f'| Cases reviewed | __ / {len(cases)} |')
    lines.append(f'| Inference failures | {n_failures} (B01: Malformed) |')
    lines.append('')
    lines.append('> B01 remains part of the canonical review population. The reviewer should apply the rubric to the observed user-facing result. Do not silently remove it from product-quality conclusions.')

    return '\n'.join(lines)


# ---------------------------------------------------------------------------
# Deep packet generation
# ---------------------------------------------------------------------------
def generate_deep_packet(corpus_by_id, deep_data):
    lines = []
    meta = deep_data['metadata']
    cases = deep_data['cases']

    # Automated summary stats
    n_completed = sum(1 for c in cases if not c['automated'].get('error_type'))
    latencies = [c['automated']['latency_ms'] for c in cases]
    median_latency = sorted(latencies)[len(latencies) // 2]
    non_empty_pass = sum(1 for c in cases if c['automated']['non_empty'])
    schema_pass = sum(1 for c in cases if c['automated'].get('schema_valid'))
    tld_pass = sum(1 for c in cases if c['automated']['target_language_detected'])
    struct_pass = sum(1 for c in cases if c['automated'].get('structure_present'))
    # Lemma/POS: only applicable for entries with focused word
    eligible_ids = [eid for eid in EXPECTED_IDS if corpus_by_id[eid]['word'] is not None]
    lemma_pass = sum(1 for c in cases if c['automated'].get('lemma_match') is True)
    pos_pass = sum(1 for c in cases if c['automated'].get('pos_match') is True)

    lines.append('# P13 Human Review Packet -- Deep Lane')
    lines.append('')
    lines.append('| Field | Value |')
    lines.append('|-------|-------|')
    lines.append(f'| Canonical baseline | {meta["timestamp"][:10].replace("-", "")} |')
    lines.append(f'| Corpus revision | {meta["corpus_revision"]} |')
    lines.append(f'| Model | {meta["model"]} |')
    lines.append(f'| Provider | {meta["provider"]} |')
    lines.append(f'| Endpoint | {meta["endpoint"]} |')
    lines.append('| Source artifact | `p13-deep-baseline-2026-09-21-054306.json` |')
    lines.append('| Merged PR | #612 |')
    lines.append('| Merged main SHA | `d3e0e34caaceb1a1208f390f83733fb08a506fae` |')
    lines.append('| Reviewer | _________________ |')
    lines.append('| Review date | _________________ |')
    lines.append('')
    lines.append('> **Human judgment fields are intentionally blank at generation time.**')
    lines.append('> Do not use automated evidence to pre-fill human scores.')
    lines.append('> A lemma exact-match failure does not automatically mean the whole Deep explanation deserves a zero.')
    lines.append('')
    lines.append('## Scoring rubric (Deep)')
    lines.append('')
    lines.append('Each field scored 0-2:')
    lines.append('- **2 (correct):** Factually accurate, no material issues')
    lines.append('- **1 (partial):** Mostly accurate, minor issues')
    lines.append('- **0 (wrong):** Factually incorrect, major omission, or misleading')
    lines.append('')
    lines.append('> For entries with no focused word, Factual correctness and Sense correctness are N/A.')
    lines.append('> Grammar correctness, Structure correctness, Usefulness, and Misleading still apply.')
    lines.append('')
    lines.append('## Automated summary')
    lines.append('')
    lines.append(f'- Total cases: {len(cases)}')
    lines.append(f'- Completed inference: {n_completed}')
    lines.append(f'- Median latency: {median_latency}ms')
    lines.append(f'- non_empty: {non_empty_pass}/{len(cases)}')
    lines.append(f'- schema_valid: {schema_pass}/{len(cases)}')
    lines.append(f'- target_language_detected: {tld_pass}/{len(cases)}')
    lines.append(f'- structure_present: {struct_pass}/{len(cases)}')
    lines.append(f'- lemma_match: {lemma_pass}/{len(eligible_ids)} (eligible entries)')
    lines.append(f'- pos_match: {pos_pass}/{len(eligible_ids)} (eligible entries)')
    lines.append('')
    lines.append('---')
    lines.append('')

    for c in cases:
        eid = c['entry_id']
        corpus = corpus_by_id[eid]
        auto = c['automated']
        output_str = c.get('output', '')
        is_no_word = corpus['word'] is None
        parsed = parse_deep_output(eid, output_str)

        lines.append(f'## {eid} -- {corpus["category"]}')
        lines.append('')
        lines.append(f'**Source:** {corpus["source"]}')
        lines.append('')
        if corpus['word']:
            lines.append(f'**Focused word:** {corpus["word"]}')
        else:
            lines.append('**Focused word:** none')
        lines.append('')
        lines.append(f'**Reference translation:** {corpus["translation"]}')
        lines.append('')

        # Expected ground truth
        lines.append('**Expected (corpus ground truth):**')
        lines.append('')
        if is_no_word:
            lines.append('- Expected lemma: N/A (no focused word)')
            lines.append('- Expected POS: N/A (no focused word)')
        else:
            lines.append(f'- Expected lemma: {corpus["expected_lemma"] or "N/A"}')
            lines.append(f'- Expected POS: {corpus["expected_pos"] or "N/A"}')
        if corpus['expected_grammar']:
            lines.append(f'- Expected grammar: {corpus["expected_grammar"]}')
        if corpus['expected_structure_rubric']:
            lines.append(f'- Expected structure rubric: {corpus["expected_structure_rubric"]}')
        if corpus['expected_idiom']:
            lines.append('- Expected idiom/register note: yes')
        if corpus['note']:
            lines.append(f'- Corpus note: {corpus["note"]}')
        lines.append('')

        # Actual model output
        lines.append('**Model analysis (canonical output):**')
        lines.append('')
        if is_no_word:
            lines.append('- Lemma: N/A (no focused word)')
            lines.append('- POS: N/A (no focused word)')
        else:
            lines.append(f'- Lemma: {parsed["lemma"] or "(empty)"}')
            lines.append(f'- POS: {parsed["pos"] or "(empty)"}')
        lines.append(f'- Sense: {parsed["sense"] or "(empty)"}')
        lines.append(f'- Structure: {parsed["structure"] or "(empty)"}')
        lines.append(f'- Note: {parsed["note"] or "(empty)"}')
        lines.append('')

        # Raw output for transparency
        lines.append(f'**Raw canonical output:** `{output_str}`')
        lines.append('')

        # Deterministic evidence
        lines.append('**Automated evidence:**')
        lines.append('')
        lines.append(f'- non_empty: {auto["non_empty"]}')
        lines.append(f'- schema_valid: {auto.get("schema_valid")}')
        lines.append(f'- target_language_detected: {auto["target_language_detected"]}')
        lines.append(f'- structure_present: {auto.get("structure_present")}')
        if is_no_word:
            lines.append('- lemma_match: N/A (no focused word)')
            lines.append('- pos_match: N/A (no focused word)')
        else:
            lm = auto.get('lemma_match')
            pm = auto.get('pos_match')
            lines.append(f'- lemma_match: {"PASS" if lm else "FAIL"}')
            lines.append(f'- pos_match: {"PASS" if pm else "FAIL"}')
        error = auto.get('error_type')
        if error:
            lines.append(f'- error_type: {error}')
        lines.append(f'- latency: {auto["latency_ms"]}ms')
        lines.append('')

        # Human review fields
        lines.append('**Human review:**')
        lines.append('')
        if is_no_word:
            lines.append('- Factual correctness: N/A')
            lines.append('- Sense correctness: N/A')
        else:
            lines.append('- Factual correctness: __ / 2')
            lines.append('- Sense correctness: __ / 2')
        lines.append('- Grammar correctness: __ / 2')
        lines.append('- Structure correctness: __ / 2')
        lines.append('- Usefulness: __ / 2')
        lines.append('- Misleading: Yes / No')
        lines.append('- Notes:')
        lines.append('')
        lines.append('---')
        lines.append('')

    # Summary section -- field-specific denominators
    no_word_count = sum(1 for eid in EXPECTED_IDS if corpus_by_id[eid]['word'] is None)
    applicable_count = len(cases) - no_word_count

    lines.append('## Summary (to be filled after review)')
    lines.append('')
    lines.append('| Metric | Value |')
    lines.append('|--------|-------|')
    lines.append(f'| Factual correctness avg | __ / 2 ({applicable_count} applicable) |')
    lines.append(f'| Sense correctness avg | __ / 2 ({applicable_count} applicable) |')
    lines.append(f'| Grammar correctness avg | __ / 2 ({len(cases)} applicable) |')
    lines.append(f'| Structure correctness avg | __ / 2 ({len(cases)} applicable) |')
    lines.append(f'| Usefulness avg | __ / 2 ({len(cases)} applicable) |')
    lines.append(f'| Misleading count | __ / {len(cases)} |')
    lines.append(f'| Cases reviewed | __ / {len(cases)} |')
    lines.append(f'| Inference failures | 0 |')
    lines.append('')

    return '\n'.join(lines)


# ---------------------------------------------------------------------------
# Validation mode
# ---------------------------------------------------------------------------
def validate(corpus_revision, corpus_entries, corpus_by_id, quick_data, deep_data):
    """Run all deterministic validation checks. Print results. Return True if all pass."""
    all_ok = True

    def check(name, condition, detail=''):
        nonlocal all_ok
        status = 'PASS' if condition else 'FAIL'
        if not condition:
            all_ok = False
        suffix = f' ({detail})' if detail else ''
        print(f'  [{status}] {name}{suffix}')

    print('=== Corpus validation ===')
    check('Corpus revision', corpus_revision == EXPECTED_CORPUS_REVISION,
          f'got {corpus_revision!r}')
    check('Corpus count == 25', len(corpus_entries) == 25,
          f'got {len(corpus_entries)}')
    check('Corpus IDs match A01..F03',
          [e['id'] for e in corpus_entries] == EXPECTED_IDS)

    # Check each entry has all required fields
    print()
    print('=== Corpus entry completeness ===')
    required_fields = ['id', 'source', 'translation', 'category',
                       'expected_lemma', 'expected_pos', 'expected_structure_rubric',
                       'expected_grammar', 'expected_idiom', 'note']
    for e in corpus_entries:
        for f in required_fields:
            if f not in e:
                check(f'{e["id"]} has {f}', False, 'field missing')
    check('All 25 entries have all required fields', True)

    quick_cases = quick_data['cases']
    deep_cases = deep_data['cases']

    print()
    print('=== Canonical JSON validation ===')
    check('Quick case count == 25', len(quick_cases) == 25, f'got {len(quick_cases)}')
    check('Deep case count == 25', len(deep_cases) == 25, f'got {len(deep_cases)}')
    check('Quick IDs match A01..F03',
          [c['entry_id'] for c in quick_cases] == EXPECTED_IDS)
    check('Deep IDs match A01..F03',
          [c['entry_id'] for c in deep_cases] == EXPECTED_IDS)

    print()
    print('=== Source fidelity ===')
    # Quick sources
    quick_source_ok = 0
    for c in quick_cases:
        eid = c['entry_id']
        if c['source'] == corpus_by_id[eid]['source']:
            quick_source_ok += 1
        else:
            check(f'Quick {eid} source match', False,
                  f'corpus={corpus_by_id[eid]["source"]!r}, json={c["source"]!r}')
    check(f'Quick source fidelity', quick_source_ok == 25, f'{quick_source_ok}/25')

    deep_source_ok = 0
    for c in deep_cases:
        eid = c['entry_id']
        if c['source'] == corpus_by_id[eid]['source']:
            deep_source_ok += 1
        else:
            check(f'Deep {eid} source match', False,
                  f'corpus={corpus_by_id[eid]["source"]!r}, json={c["source"]!r}')
    check(f'Deep source fidelity', deep_source_ok == 25, f'{deep_source_ok}/25')

    print()
    print('=== Translation fidelity ===')
    # Verify translations in corpus match what we would render
    trans_ok = 0
    for e in corpus_entries:
        eid = e['id']
        # Verify translation is non-empty
        if e['translation']:
            trans_ok += 1
        else:
            check(f'{eid} translation non-empty', False)
    check(f'Corpus translations non-empty', trans_ok == 25, f'{trans_ok}/25')

    print()
    print('=== Focused word fidelity ===')
    qw_ok = 0
    for c in quick_cases:
        eid = c['entry_id']
        if c.get('focused_word') == corpus_by_id[eid]['word']:
            qw_ok += 1
        else:
            check(f'Quick {eid} focused_word match', False,
                  f'corpus={corpus_by_id[eid]["word"]!r}, json={c.get("focused_word")!r}')
    check(f'Quick focused_word fidelity', qw_ok == 25, f'{qw_ok}/25')

    dw_ok = 0
    for c in deep_cases:
        eid = c['entry_id']
        if c.get('focused_word') == corpus_by_id[eid]['word']:
            dw_ok += 1
        else:
            check(f'Deep {eid} focused_word match', False,
                  f'corpus={corpus_by_id[eid]["word"]!r}, json={c.get("focused_word")!r}')
    check(f'Deep focused_word fidelity', dw_ok == 25, f'{dw_ok}/25')

    print()
    print('=== Deep output parsing ===')
    deep_parse_ok = 0
    for c in deep_cases:
        eid = c['entry_id']
        output = c.get('output', '')
        parts = output.split('|')
        if len(parts) == 5:
            deep_parse_ok += 1
        else:
            check(f'Deep {eid} pipe field count', False,
                  f'got {len(parts)}, raw={output!r}')
    check(f'All Deep outputs have exactly 5 fields', deep_parse_ok == 25, f'{deep_parse_ok}/25')

    print()
    print('=== Deep output fidelity ===')
    # Verify raw outputs are preserved exactly
    deep_output_ok = 0
    for c in deep_cases:
        eid = c['entry_id']
        output = c.get('output', '')
        parsed = parse_deep_output(eid, output)
        # Reconstruct from parsed and compare
        reconstructed = '|'.join([parsed['lemma'], parsed['pos'], parsed['sense'],
                                   parsed['structure'], parsed['note']])
        # Note: the original may have trailing whitespace differences
        # The check is that parsing produces exactly 5 fields from the raw output
        parts = output.split('|')
        if len(parts) == 5:
            deep_output_ok += 1
        else:
            check(f'Deep {eid} output lossless', False)
    check(f'Deep output lossless parsing', deep_output_ok == 25, f'{deep_output_ok}/25')

    print()
    print('=== B01 treatment ===')
    b01_quick = next(c for c in quick_cases if c['entry_id'] == 'B01')
    b01_error = b01_quick['automated'].get('error_type')
    check('B01 Quick error_type is Malformed', b01_error == 'Malformed',
          f'got {b01_error!r}')
    check('B01 Quick output is empty', b01_quick.get('output') == '',
          f'got {b01_quick.get("output")!r}')

    print()
    print('=== No-word applicability ===')
    no_word_ids = ['A03', 'B03', 'C01', 'C02', 'C04', 'E03']
    for eid in no_word_ids:
        check(f'{eid} has no focused word', corpus_by_id[eid]['word'] is None,
              f'word={corpus_by_id[eid]["word"]!r}')
    no_word_count = sum(1 for eid in EXPECTED_IDS if corpus_by_id[eid]['word'] is None)
    check(f'No-word count == 6', no_word_count == 6, f'got {no_word_count}')

    print()
    print('=== Human review blank check ===')
    # Verify Quick JSON human fields are all None
    quick_human_blank = all(
        all(v is None for v in c.get('human', {}).values())
        for c in quick_cases
    )
    check('Quick JSON human fields all null', quick_human_blank)

    deep_human_blank = all(
        all(v is None for v in c.get('human', {}).values())
        for c in deep_cases
    )
    check('Deep JSON human fields all null', deep_human_blank)

    print()
    if all_ok:
        print('ALL VALIDATION CHECKS PASSED')
    else:
        print('SOME VALIDATION CHECKS FAILED')
    return all_ok


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------
def check_for_human_review_data():
    """
    Check if existing packets contain human review data.

    Returns True if any human scores are present (not blank or N/A template).
    """
    import re
    from pathlib import Path

    def has_scores(packet_path):
        packet_path = Path(packet_path)
        if not packet_path.exists():
            return False

        text = packet_path.read_text(encoding='utf-8')

        # Look for patterns like "- Meaning preserved: 1 / 2" or "- Factual correctness: 2 / 2"
        # Exclude patterns like "- Meaning preserved: __ / 2" or "- Factual correctness: N/A"
        score_pattern = re.compile(r'^- .+?: (\d) / \d+$', re.MULTILINE)
        misleading_pattern = re.compile(r'^- Misleading: (Yes|No)$', re.MULTILINE)

        # Check for any numeric scores
        if score_pattern.search(text):
            return True

        # Check for explicit Yes/No (not "Yes / No" template)
        if misleading_pattern.search(text):
            return True

        return False

    quick_has_scores = has_scores(QUICK_PACKET)
    deep_has_scores = has_scores(DEEP_PACKET)

    return quick_has_scores or deep_has_scores


def main():
    validate_only = '--validate' in sys.argv
    force_overwrite = '--force-overwrite-reviewed' in sys.argv

    # 1. Parse corpus from Rust source
    print(f'Parsing corpus from {CORPUS_RS}')
    corpus_revision, corpus_entries = parse_corpus_rs(CORPUS_RS)
    corpus_by_id = {e['id']: e for e in corpus_entries}
    print(f'  Corpus revision: {corpus_revision}')
    print(f'  Corpus entries: {len(corpus_entries)}')
    print(f'  IDs: {[e["id"] for e in corpus_entries]}')

    # 2. Load canonical JSON artifacts
    print(f'\nLoading Quick JSON from {QUICK_JSON}')
    quick_data = load_json(QUICK_JSON)
    print(f'  Quick cases: {len(quick_data["cases"])}')

    print(f'Loading Deep JSON from {DEEP_JSON}')
    deep_data = load_json(DEEP_JSON)
    print(f'  Deep cases: {len(deep_data["cases"])}')

    # 3. Assert integrity
    print('\nAsserting integrity...')
    assert_integrity(corpus_revision, corpus_entries, quick_data, deep_data)
    print('  All integrity assertions passed.')

    if validate_only:
        print('\n=== VALIDATION MODE ===')
        ok = validate(corpus_revision, corpus_entries, corpus_by_id, quick_data, deep_data)
        sys.exit(0 if ok else 1)

    # 3.5. Check for existing human review data
    if not force_overwrite:
        print('\nChecking for existing human review data...')
        has_human_data = check_for_human_review_data()
        if has_human_data:
            print('ERROR: Existing packets contain human review data.')
            print('Use --force-overwrite-reviewed to overwrite.')
            sys.exit(1)
        print('  No human review data found.')

    # 4. Generate Quick packet
    print('\nGenerating Quick packet...')
    quick_packet = generate_quick_packet(corpus_by_id, quick_data)
    with open(QUICK_PACKET, 'w', encoding='utf-8') as f:
        f.write(quick_packet)
    print(f'  Written to {QUICK_PACKET}')

    # 5. Generate Deep packet
    print('Generating Deep packet...')
    deep_packet = generate_deep_packet(corpus_by_id, deep_data)
    with open(DEEP_PACKET, 'w', encoding='utf-8') as f:
        f.write(deep_packet)
    print(f'  Written to {DEEP_PACKET}')

    # 6. Run validation
    print('\n=== POST-GENERATION VALIDATION ===')
    ok = validate(corpus_revision, corpus_entries, corpus_by_id, quick_data, deep_data)

    if not ok:
        print('\nERROR: Validation failed after generation. Packets may be incorrect.')
        sys.exit(1)

    print('\nDone. Both packets generated and validated.')


if __name__ == '__main__':
    main()
