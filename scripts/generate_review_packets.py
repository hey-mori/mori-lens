"""Generate P13 human review packets from canonical JSON and corpus."""
import json

# Corpus data from eval_corpus.rs (canary-25-v1)
CORPUS = {
    'A01': {'source': 'The pointer rests on the sentence.', 'translation': '포인터가 문장 위에 멈추니다.', 'word': 'rests', 'category': 'general', 'expected_lemma': 'rest', 'expected_pos': '동사', 'expected_grammar': 'simple present tense', 'expected_structure_rubric': None, 'expected_idiom': False, 'note': 'simple sentence, one clause'},
    'A02': {'source': 'The card appears when the pointer settles, which takes a moment.', 'translation': '포인터가 안정되면 카드가 나타나는데, 잠시 걸립니다.', 'word': 'settles', 'category': 'general', 'expected_lemma': 'settle', 'expected_pos': '동사', 'expected_grammar': 'complex sentence with temporal clause', 'expected_structure_rubric': 'subordinate clause', 'expected_idiom': False, 'note': 'two clauses: temporal + relative'},
    'A03': {'source': 'The acquirer reads the full sentence rather than a single character.', 'translation': '획득기는 한 글자 대신 전체 문장을 읽습니다.', 'word': None, 'category': 'general', 'expected_lemma': None, 'expected_pos': None, 'expected_grammar': 'rather than contrast', 'expected_structure_rubric': 'comparison', 'expected_idiom': False, 'note': 'compound sentence, no focused word'},
    'A04': {'source': 'A card that the model has already answered is served from cache.', 'translation': '모델이 이미 답한 카드는 캐시에서 제공됩니다.', 'word': 'answered', 'category': 'general', 'expected_lemma': 'answer', 'expected_pos': '동사', 'expected_grammar': 'present perfect in relative clause', 'expected_structure_rubric': 'relative clause', 'expected_idiom': False, 'note': 'passive main clause with relative clause'},
    'A05': {'source': 'If the model is unavailable, the card says so.', 'translation': '모델을 사용할 수 없으면 카드가 그렇게 알립니다.', 'word': 'unavailable', 'category': 'general', 'expected_lemma': 'unavailable', 'expected_pos': '형용사', 'expected_grammar': 'zero conditional', 'expected_structure_rubric': 'conditional', 'expected_idiom': False, 'note': 'if-then structure'},
    'B01': {'source': 'The lease has expired and the model is no longer resident.', 'translation': '리스가 만료되어 모델이 더 이상 상주하지 않습니다.', 'word': 'expired', 'category': 'grammar', 'expected_lemma': 'expire', 'expected_pos': '동사', 'expected_grammar': 'present perfect', 'expected_structure_rubric': 'compound', 'expected_idiom': False, 'note': "present perfect with 'and' coordination"},
    'B02': {'source': 'I would rather it failed loudly than passed silently.', 'translation': '조용히 통과하는 것보다 크게 실패하는 것이 낫겠습니다.', 'word': 'failed', 'category': 'grammar', 'expected_lemma': 'fail', 'expected_pos': '동사', 'expected_grammar': 'subjunctive with would rather', 'expected_structure_rubric': 'comparison', 'expected_idiom': False, 'note': "subjunctive mood after 'would rather'"},
    'B03': {'source': 'Not only does the model translate, it also explains.', 'translation': '모델은 번역할 뿐만 아니라 설명도 합니다.', 'word': None, 'category': 'grammar', 'expected_lemma': None, 'expected_pos': None, 'expected_grammar': "negative inversion with 'not only'", 'expected_structure_rubric': 'inversion', 'expected_idiom': False, 'note': 'fronted negative triggers subject-auxiliary inversion'},
    'B04': {'source': 'Placed on the wrong sentence, the card is worse than none.', 'translation': '잘못된 문장에 배치되면 카드는 없는 것만 못합니다.', 'word': 'Placed', 'category': 'grammar', 'expected_lemma': 'place', 'expected_pos': '분사', 'expected_grammar': 'past participle as reduced adverbial', 'expected_structure_rubric': 'reduced clause', 'expected_idiom': False, 'note': 'reduced adverbial clause'},
    'B05': {'source': 'The reader asked for explaining the sentence.', 'translation': '독자가 문장 설명을 요청했습니다.', 'word': 'explaining', 'category': 'grammar', 'expected_lemma': 'explain', 'expected_pos': '동명사', 'expected_grammar': 'gerund as object of preposition', 'expected_structure_rubric': None, 'expected_idiom': False, 'note': "gerund after preposition 'for'"},
    'C01': {'source': 'The output files came back empty.', 'translation': '출력 파일이 비어 있었습니다.', 'word': None, 'category': 'idiom', 'expected_lemma': None, 'expected_pos': None, 'expected_grammar': "phrasal verb 'come back'", 'expected_structure_rubric': None, 'expected_idiom': True, 'note': 'phrasal verb: come back = return (with result)'},
    'C02': {'source': 'The model is up to date with the latest release.', 'translation': '모델이 최신 릴리즈에 맞게 업데이트되어 있습니다.', 'word': None, 'category': 'idiom', 'expected_lemma': None, 'expected_pos': None, 'expected_grammar': "adjective idiom 'up to date'", 'expected_structure_rubric': None, 'expected_idiom': True, 'note': 'compound adjective: up to date = current'},
    'C03': {'source': 'The build is green.', 'translation': '빌드가 성공입니다.', 'word': 'green', 'category': 'idiom', 'expected_lemma': 'green', 'expected_pos': '형용사', 'expected_grammar': 'domain-specific adjective use', 'expected_structure_rubric': None, 'expected_idiom': True, 'note': 'CI/CD jargon: green = all tests passed'},
    'C04': {'source': 'This is a no-op for existing sessions.', 'translation': '기존 세션에 대해 이것은 아무 작업도 하지 않습니다.', 'word': None, 'category': 'idiom', 'expected_lemma': None, 'expected_pos': None, 'expected_grammar': 'technical compound noun', 'expected_structure_rubric': None, 'expected_idiom': True, 'note': 'tech jargon: no-op = no operation'},
    'D01': {'source': 'Error: the adapter returned null for a non-optional field.', 'translation': '오류: 어댑터가 필수 필드에 null을 반환했습니다.', 'word': 'adapter', 'category': 'developer', 'expected_lemma': 'adapter', 'expected_pos': '명사', 'expected_grammar': 'error message structure', 'expected_structure_rubric': None, 'expected_idiom': False, 'note': 'developer context: adapter pattern'},
    'D02': {'source': 'Returns the nearest text range to a screen point.', 'translation': '화면 지점에 가장 가까운 텍스트 범위를 반환합니다.', 'word': 'nearest', 'category': 'developer', 'expected_lemma': 'near', 'expected_pos': '형용사', 'expected_grammar': 'superlative adjective', 'expected_structure_rubric': None, 'expected_idiom': False, 'note': 'API documentation style'},
    'D03': {'source': 'This change removes an assumption the code was built on.', 'translation': '이 변경은 코드가 기반으로 했던 가정을 제거합니다.', 'word': 'assumption', 'category': 'developer', 'expected_lemma': 'assumption', 'expected_pos': '명사', 'expected_grammar': 'reduced relative clause', 'expected_structure_rubric': 'relative clause', 'expected_idiom': False, 'note': "code review context, reduced relative 'the code was built on'"},
    'D04': {'source': 'Acquisition is solved and targeting is not.', 'translation': '획득은 해결되었고 타기팅은 아직 아닙니다.', 'word': 'targeting', 'category': 'developer', 'expected_lemma': 'targeting', 'expected_pos': '명사', 'expected_grammar': 'parallel structure with ellipsis', 'expected_structure_rubric': 'ellipsis', 'expected_idiom': False, 'note': 'architecture discussion, gapped coordination'},
    'D05': {'source': 'The token is valid but the lease has expired.', 'translation': '토큰은 유효하지만 리스가 만료되었습니다.', 'word': 'lease', 'category': 'developer', 'expected_lemma': 'lease', 'expected_pos': '명사', 'expected_grammar': 'present perfect with contrast', 'expected_structure_rubric': 'contrast', 'expected_idiom': False, 'note': "debugging context: 'lease' is a model retention term"},
    'E01': {'source': 'Were the model to fail, the fallback would engage immediately.', 'translation': '모델이 실패했다면 폴백이 즉시 작동했을 것입니다.', 'word': 'fail', 'category': 'exam', 'expected_lemma': 'fail', 'expected_pos': '동사', 'expected_grammar': 'inverted second conditional (were...to)', 'expected_structure_rubric': 'inverted conditional', 'expected_idiom': False, 'note': "formal conditional without 'if', common in exam passages"},
    'E02': {'source': 'The data, having been processed, was written to the output.', 'translation': '데이터는 처리된 후 출력에 기록되었습니다.', 'word': 'processed', 'category': 'exam', 'expected_lemma': 'process', 'expected_pos': '분사', 'expected_grammar': 'perfect participle passive (having been + pp)', 'expected_structure_rubric': 'perfect participle', 'expected_idiom': False, 'note': 'formal perfect participle, common in exam reading'},
    'E03': {'source': 'So quickly did the model respond that nobody noticed the latency.', 'translation': '모델이 너무나 빨리 응답해서 아무도 지연을 눈치찰 못했습니다.', 'word': None, 'category': 'exam', 'expected_lemma': None, 'expected_pos': None, 'expected_grammar': 'so...that with fronted adverb and inversion', 'expected_structure_rubric': 'so...that inversion', 'expected_idiom': False, 'note': 'emphatic inversion pattern, exam favourite'},
    'F01': {'source': 'vacuously', 'translation': '공허하게', 'word': 'vacuously', 'category': 'edge', 'expected_lemma': 'vacuously', 'expected_pos': '부사', 'expected_grammar': 'single adverb, no structure', 'expected_structure_rubric': None, 'expected_idiom': False, 'note': 'single word: structure must be empty, lemma is the word itself'},
    'F02': {'source': 'The Workloom dashboard shows one row per session.', 'translation': 'Workloom 대시보드는 세션당 한 줄을 표시합니다.', 'word': 'Workloom', 'category': 'edge', 'expected_lemma': 'Workloom', 'expected_pos': '명사', 'expected_grammar': 'proper noun', 'expected_structure_rubric': None, 'expected_idiom': False, 'note': 'proper noun: lemma should not change the form'},
    'F03': {'source': 'The timeout is 404ms, which exceeds the 200ms budget.', 'translation': '타임아웃은 404ms로, 200ms 예산을 초과합니다.', 'word': 'timeout', 'category': 'edge', 'expected_lemma': 'timeout', 'expected_pos': '명사', 'expected_grammar': 'numeric expression with relative clause', 'expected_structure_rubric': 'relative clause', 'expected_idiom': False, 'note': 'numbers and units in technical context'},
}

IDS = ['A01','A02','A03','A04','A05','B01','B02','B03','B04','B05',
       'C01','C02','C03','C04','D01','D02','D03','D04','D05',
       'E01','E02','E03','F01','F02','F03']


def generate_quick_packet():
    with open('docs/benchmarks/p13-quick-baseline-2026-09-21-054306.json', encoding='utf-8') as f:
        data = json.load(f)
    cases = {c['entry_id']: c for c in data['cases']}

    lines = []
    lines.append('# P13 Human Review Packet -- Quick Lane')
    lines.append('')
    lines.append('| Field | Value |')
    lines.append('|-------|-------|')
    lines.append('| Canonical baseline | 2026-09-21-054306 |')
    lines.append('| Corpus revision | canary-25-v1 |')
    lines.append('| Model | qwen2.5:7b |')
    lines.append('| Provider | Ollama |')
    lines.append('| Endpoint | 127.0.0.1:11434 |')
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
    lines.append('- Total cases: 25')
    lines.append('- Completed inference: 24')
    lines.append('- Canonical failure: B01 `Malformed`')
    lines.append('- Median latency: 1538ms')
    lines.append('- non_empty: 96%, target_language_detected: 96%')
    lines.append('')
    lines.append('> The first measured request (A01, 14561ms) includes cold-start cost. Median represents steady-state.')
    lines.append('')
    lines.append('---')
    lines.append('')

    for eid in IDS:
        c = CORPUS[eid]
        q = cases[eid]
        auto = q['automated']
        obs = q.get('observations', {})

        lines.append(f'## {eid} -- {c["category"]}')
        lines.append('')
        lines.append(f'**Source:** {c["source"]}')
        lines.append('')
        if c['word']:
            lines.append(f'**Focused word:** {c["word"]}')
            lines.append('')
        lines.append(f'**Reference translation:** {c["translation"]}')
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
            lines.append(f'**Model output:** {q["output"]}')

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

    lines.append('## Summary (to be filled after review)')
    lines.append('')
    lines.append('| Metric | Value |')
    lines.append('|--------|-------|')
    lines.append('| Meaning preservation avg | __ / 2 |')
    lines.append('| Naturalness avg | __ / 2 |')
    lines.append('| Completeness avg | __ / 2 |')
    lines.append('| Cases reviewed | __ / 25 |')
    lines.append('| Inference failures (B01) | 1 (not scored) |')
    lines.append('')
    lines.append('> Inference failures are part of user-facing quality evidence. Do not silently drop B01 from product-quality conclusions.')

    with open('docs/benchmarks/p13-human-review-quick-2026-09-21.md', 'w', encoding='utf-8') as f:
        f.write('\n'.join(lines))
    print(f'Quick packet: {len(IDS)} entries written')


def parse_deep_output(output_str):
    """Parse the pipe-separated Deep output format: lemma|POS|sense|structure|note"""
    parts = output_str.split('|')
    result = {
        'lemma': parts[0].strip() if len(parts) > 0 else '',
        'pos': parts[1].strip() if len(parts) > 1 else '',
        'sense': parts[2].strip() if len(parts) > 2 else '',
        'structure': parts[3].strip() if len(parts) > 3 else '',
        'note': parts[4].strip() if len(parts) > 4 else '',
    }
    return result


def generate_deep_packet():
    with open('docs/benchmarks/p13-deep-baseline-2026-09-21-054306.json', encoding='utf-8') as f:
        data = json.load(f)
    cases = {c['entry_id']: c for c in data['cases']}

    lines = []
    lines.append('# P13 Human Review Packet -- Deep Lane')
    lines.append('')
    lines.append('| Field | Value |')
    lines.append('|-------|-------|')
    lines.append('| Canonical baseline | 2026-09-21-054306 |')
    lines.append('| Corpus revision | canary-25-v1 |')
    lines.append('| Model | qwen2.5:7b |')
    lines.append('| Provider | Ollama |')
    lines.append('| Endpoint | 127.0.0.1:11434 |')
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
    lines.append('## Automated summary')
    lines.append('')
    lines.append('- Total cases: 25')
    lines.append('- Completed inference: 25 (zero inference failures)')
    lines.append('- Median latency: 7526ms')
    lines.append('- non_empty: 100%, schema_valid: 100%, target_language_detected: 100%')
    lines.append('- structure_present: 96% (24/25)')
    lines.append('- lemma_match: 42.1% (8/19 eligible)')
    lines.append('- pos_match: 84.2% (16/19 eligible)')
    lines.append('')
    lines.append('---')
    lines.append('')

    for eid in IDS:
        c = CORPUS[eid]
        d = cases[eid]
        auto = d['automated']
        output_str = d.get('output', '')
        parsed = parse_deep_output(output_str)
        is_no_word = c['word'] is None

        lines.append(f'## {eid} -- {c["category"]}')
        lines.append('')
        lines.append(f'**Source:** {c["source"]}')
        lines.append('')
        if c['word']:
            lines.append(f'**Focused word:** {c["word"]}')
        else:
            lines.append('**Focused word:** none')
        lines.append('')
        lines.append(f'**Reference translation:** {c["translation"]}')
        lines.append('')

        # Expected ground truth
        lines.append('**Expected (corpus ground truth):**')
        lines.append('')
        if is_no_word:
            lines.append('- Expected lemma: N/A (no focused word)')
            lines.append('- Expected POS: N/A (no focused word)')
        else:
            lines.append(f'- Expected lemma: {c["expected_lemma"] or "N/A"}')
            lines.append(f'- Expected POS: {c["expected_pos"] or "N/A"}')
        if c['expected_grammar']:
            lines.append(f'- Expected grammar: {c["expected_grammar"]}')
        if c['expected_structure_rubric']:
            lines.append(f'- Expected structure rubric: {c["expected_structure_rubric"]}')
        if c['expected_idiom']:
            lines.append('- Expected idiom/register note: yes')
        if c['note']:
            lines.append(f'- Corpus note: {c["note"]}')
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
        lines.append(f'- schema_valid: {auto["schema_valid"]}')
        lines.append(f'- target_language_detected: {auto["target_language_detected"]}')
        lines.append(f'- structure_present: {auto["structure_present"]}')
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

    lines.append('## Summary (to be filled after review)')
    lines.append('')
    lines.append('| Metric | Value |')
    lines.append('|--------|-------|')
    lines.append('| Factual correctness avg | __ / 2 |')
    lines.append('| Sense correctness avg | __ / 2 |')
    lines.append('| Grammar correctness avg | __ / 2 |')
    lines.append('| Structure correctness avg | __ / 2 |')
    lines.append('| Usefulness avg | __ / 2 |')
    lines.append('| Misleading count | __ |')
    lines.append('| Cases reviewed | __ / 25 |')
    lines.append('| Inference failures | 0 |')
    lines.append('')

    with open('docs/benchmarks/p13-human-review-deep-2026-09-21.md', 'w', encoding='utf-8') as f:
        f.write('\n'.join(lines))
    print(f'Deep packet: {len(IDS)} entries written')


if __name__ == '__main__':
    generate_quick_packet()
    generate_deep_packet()
