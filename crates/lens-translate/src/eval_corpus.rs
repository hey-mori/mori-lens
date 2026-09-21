//! Shared evaluation corpus for P13 benchmark and deep analysis evaluation.
//!
//! This module contains the canonical 25-entry hand-curated corpus used by:
//! - `deep_eval.rs` for deep analysis quality evaluation
//! - `p13-live-baseline` for production benchmark execution
//!
//! # Ground truth
//!
//! The expected values are written by a person, not by a model. A lemma is
//! checked by exact match (dictionary forms are deterministic). A part of
//! speech is checked by keyword containment. Everything else, especially
//! grammar explanations and clause structures, is left for a human to score.
//!
//! # Corpus revision
//!
//! The corpus revision is `canary-25-v1` with source SHA traceable via git.
//! All entries use stable IDs (A01-F03) and preserve historical ground truth.

/// One sentence the evaluation harness knows the answer to.
///
/// Not all fields apply to every entry. The evaluation checks only the
/// fields whose `expected_*` is `Some`.
#[derive(Debug, Clone)]
pub struct CorpusEntry {
    /// Short stable identifier: "A01", "B03", "D07".
    pub id: &'static str,
    /// The English source sentence.
    pub source: &'static str,
    /// Hand-written Korean translation (not model-generated).
    pub translation: &'static str,
    /// The word the pointer rests on, or None for sentence-only analysis.
    pub word: Option<&'static str>,
    /// Category label for reporting.
    pub category: &'static str,
    /// Expected dictionary form of the focused word (exact match).
    pub expected_lemma: Option<&'static str>,
    /// Expected part of speech keywords (Korean, containment match).
    pub expected_pos: Option<&'static str>,
    /// What a good clause structure explanation should mention.
    pub expected_structure_rubric: Option<&'static str>,
    /// Grammar phenomenon being tested (English label for reporting).
    pub expected_grammar: Option<&'static str>,
    /// Whether an idiom or register note is expected.
    pub expected_idiom: Option<bool>,
    /// Free-form note about what makes this entry interesting.
    pub note: Option<&'static str>,
}

/// The evaluation corpus: 25 entries across 6 categories.
///
/// All sentences use this repository's own domain vocabulary.
/// Korean translations are hand-written, not model-generated.
///
/// Categories:
/// - A. General English (5): A01-A05
/// - B. Grammar-sensitive (5): B01-B05
/// - C. Idiom / expression (4): C01-C04
/// - D. Developer (5): D01-D05
/// - E. Exam (3): E01-E03
/// - F. Edge cases (3): F01-F03
pub static CORPUS: &[CorpusEntry] = &[
    // ---- A. General English (5) ----
    CorpusEntry {
        id: "A01",
        source: "The pointer rests on the sentence.",
        translation: "포인터가 문장 위에 멈춥니다.",
        word: Some("rests"),
        category: "general",
        expected_lemma: Some("rest"),
        expected_pos: Some("동사"),
        expected_structure_rubric: None,
        expected_grammar: Some("simple present tense"),
        expected_idiom: Some(false),
        note: Some("simple sentence, one clause"),
    },
    CorpusEntry {
        id: "A02",
        source: "The card appears when the pointer settles, which takes a moment.",
        translation: "포인터가 안정되면 카드가 나타나는데, 잠시 걸립니다.",
        word: Some("settles"),
        category: "general",
        expected_lemma: Some("settle"),
        expected_pos: Some("동사"),
        expected_structure_rubric: Some("subordinate clause"),
        expected_grammar: Some("complex sentence with temporal clause"),
        expected_idiom: Some(false),
        note: Some("two clauses: temporal + relative"),
    },
    CorpusEntry {
        id: "A03",
        source: "The acquirer reads the full sentence rather than a single character.",
        translation: "획득기는 한 글자 대신 전체 문장을 읽습니다.",
        word: None,
        category: "general",
        expected_lemma: None,
        expected_pos: None,
        expected_structure_rubric: Some("comparison"),
        expected_grammar: Some("rather than contrast"),
        expected_idiom: Some(false),
        note: Some("compound sentence, no focused word"),
    },
    CorpusEntry {
        id: "A04",
        source: "A card that the model has already answered is served from cache.",
        translation: "모델이 이미 답한 카드는 캐시에서 제공됩니다.",
        word: Some("answered"),
        category: "general",
        expected_lemma: Some("answer"),
        expected_pos: Some("동사"),
        expected_structure_rubric: Some("relative clause"),
        expected_grammar: Some("present perfect in relative clause"),
        expected_idiom: Some(false),
        note: Some("passive main clause with relative clause"),
    },
    CorpusEntry {
        id: "A05",
        source: "If the model is unavailable, the card says so.",
        translation: "모델을 사용할 수 없으면 카드가 그렇게 알립니다.",
        word: Some("unavailable"),
        category: "general",
        expected_lemma: Some("unavailable"),
        expected_pos: Some("형용사"),
        expected_structure_rubric: Some("conditional"),
        expected_grammar: Some("zero conditional"),
        expected_idiom: Some(false),
        note: Some("if-then structure"),
    },
    // ---- B. Grammar-sensitive (5) ----
    CorpusEntry {
        id: "B01",
        source: "The lease has expired and the model is no longer resident.",
        translation: "리스가 만료되어 모델이 더 이상 상주하지 않습니다.",
        word: Some("expired"),
        category: "grammar",
        expected_lemma: Some("expire"),
        expected_pos: Some("동사"),
        expected_structure_rubric: Some("compound"),
        expected_grammar: Some("present perfect"),
        expected_idiom: Some(false),
        note: Some("present perfect with 'and' coordination"),
    },
    CorpusEntry {
        id: "B02",
        source: "I would rather it failed loudly than passed silently.",
        translation: "조용히 통과하는 것보다 크게 실패하는 것이 낫겠습니다.",
        word: Some("failed"),
        category: "grammar",
        expected_lemma: Some("fail"),
        expected_pos: Some("동사"),
        expected_structure_rubric: Some("comparison"),
        expected_grammar: Some("subjunctive with would rather"),
        expected_idiom: Some(false),
        note: Some("subjunctive mood after 'would rather'"),
    },
    CorpusEntry {
        id: "B03",
        source: "Not only does the model translate, it also explains.",
        translation: "모델은 번역할 뿐만 아니라 설명도 합니다.",
        word: None,
        category: "grammar",
        expected_lemma: None,
        expected_pos: None,
        expected_structure_rubric: Some("inversion"),
        expected_grammar: Some("negative inversion with 'not only'"),
        expected_idiom: Some(false),
        note: Some("fronted negative triggers subject-auxiliary inversion"),
    },
    CorpusEntry {
        id: "B04",
        source: "Placed on the wrong sentence, the card is worse than none.",
        translation: "잘못된 문장에 배치되면 카드는 없는 것만 못합니다.",
        word: Some("Placed"),
        category: "grammar",
        expected_lemma: Some("place"),
        expected_pos: Some("분사"),
        expected_structure_rubric: Some("reduced clause"),
        expected_grammar: Some("past participle as reduced adverbial"),
        expected_idiom: Some(false),
        note: Some("reduced adverbial clause"),
    },
    CorpusEntry {
        id: "B05",
        source: "The reader asked for explaining the sentence.",
        translation: "독자가 문장 설명을 요청했습니다.",
        word: Some("explaining"),
        category: "grammar",
        expected_lemma: Some("explain"),
        expected_pos: Some("동명사"),
        expected_structure_rubric: None,
        expected_grammar: Some("gerund as object of preposition"),
        expected_idiom: Some(false),
        note: Some("gerund after preposition 'for'"),
    },
    // ---- C. Idiom / expression (4) ----
    CorpusEntry {
        id: "C01",
        source: "The output files came back empty.",
        translation: "출력 파일이 비어 있었습니다.",
        word: None,
        category: "idiom",
        expected_lemma: None,
        expected_pos: None,
        expected_structure_rubric: None,
        expected_grammar: Some("phrasal verb 'come back'"),
        expected_idiom: Some(true),
        note: Some("phrasal verb: come back = return (with result)"),
    },
    CorpusEntry {
        id: "C02",
        source: "The model is up to date with the latest release.",
        translation: "모델이 최신 릴리스에 맞게 업데이트되어 있습니다.",
        word: None,
        category: "idiom",
        expected_lemma: None,
        expected_pos: None,
        expected_structure_rubric: None,
        expected_grammar: Some("adjective idiom 'up to date'"),
        expected_idiom: Some(true),
        note: Some("compound adjective: up to date = current"),
    },
    CorpusEntry {
        id: "C03",
        source: "The build is green.",
        translation: "빌드가 성공입니다.",
        word: Some("green"),
        category: "idiom",
        expected_lemma: Some("green"),
        expected_pos: Some("형용사"),
        expected_structure_rubric: None,
        expected_grammar: Some("domain-specific adjective use"),
        expected_idiom: Some(true),
        note: Some("CI/CD jargon: green = all tests passed"),
    },
    CorpusEntry {
        id: "C04",
        source: "This is a no-op for existing sessions.",
        translation: "기존 세션에 대해 이것은 아무 작업도 하지 않습니다.",
        word: None,
        category: "idiom",
        expected_lemma: None,
        expected_pos: None,
        expected_structure_rubric: None,
        expected_grammar: Some("technical compound noun"),
        expected_idiom: Some(true),
        note: Some("tech jargon: no-op = no operation"),
    },
    // ---- D. Developer (5) ----
    CorpusEntry {
        id: "D01",
        source: "Error: the adapter returned null for a non-optional field.",
        translation: "오류: 어댑터가 필수 필드에 null을 반환했습니다.",
        word: Some("adapter"),
        category: "developer",
        expected_lemma: Some("adapter"),
        expected_pos: Some("명사"),
        expected_structure_rubric: None,
        expected_grammar: Some("error message structure"),
        expected_idiom: Some(false),
        note: Some("developer context: adapter pattern"),
    },
    CorpusEntry {
        id: "D02",
        source: "Returns the nearest text range to a screen point.",
        translation: "화면 지점에 가장 가까운 텍스트 범위를 반환합니다.",
        word: Some("nearest"),
        category: "developer",
        expected_lemma: Some("near"),
        expected_pos: Some("형용사"),
        expected_structure_rubric: None,
        expected_grammar: Some("superlative adjective"),
        expected_idiom: Some(false),
        note: Some("API documentation style"),
    },
    CorpusEntry {
        id: "D03",
        source: "This change removes an assumption the code was built on.",
        translation: "이 변경은 코드가 기반으로 했던 가정을 제거합니다.",
        word: Some("assumption"),
        category: "developer",
        expected_lemma: Some("assumption"),
        expected_pos: Some("명사"),
        expected_structure_rubric: Some("relative clause"),
        expected_grammar: Some("reduced relative clause"),
        expected_idiom: Some(false),
        note: Some("code review context, reduced relative 'the code was built on'"),
    },
    CorpusEntry {
        id: "D04",
        source: "Acquisition is solved and targeting is not.",
        translation: "획득은 해결되었고 타기팅은 아직 아닙니다.",
        word: Some("targeting"),
        category: "developer",
        expected_lemma: Some("targeting"),
        expected_pos: Some("명사"),
        expected_structure_rubric: Some("ellipsis"),
        expected_grammar: Some("parallel structure with ellipsis"),
        expected_idiom: Some(false),
        note: Some("architecture discussion, gapped coordination"),
    },
    CorpusEntry {
        id: "D05",
        source: "The token is valid but the lease has expired.",
        translation: "토큰은 유효하지만 리스가 만료되었습니다.",
        word: Some("lease"),
        category: "developer",
        expected_lemma: Some("lease"),
        expected_pos: Some("명사"),
        expected_structure_rubric: Some("contrast"),
        expected_grammar: Some("present perfect with contrast"),
        expected_idiom: Some(false),
        note: Some("debugging context: 'lease' is a model retention term"),
    },
    // ---- E. Exam (3) ----
    CorpusEntry {
        id: "E01",
        source: "Were the model to fail, the fallback would engage immediately.",
        translation: "모델이 실패했다면 폴백이 즉시 작동했을 것입니다.",
        word: Some("fail"),
        category: "exam",
        expected_lemma: Some("fail"),
        expected_pos: Some("동사"),
        expected_structure_rubric: Some("inverted conditional"),
        expected_grammar: Some("inverted second conditional (were...to)"),
        expected_idiom: Some(false),
        note: Some("formal conditional without 'if', common in exam passages"),
    },
    CorpusEntry {
        id: "E02",
        source: "The data, having been processed, was written to the output.",
        translation: "데이터는 처리된 후 출력에 기록되었습니다.",
        word: Some("processed"),
        category: "exam",
        expected_lemma: Some("process"),
        expected_pos: Some("분사"),
        expected_structure_rubric: Some("perfect participle"),
        expected_grammar: Some("perfect participle passive (having been + pp)"),
        expected_idiom: Some(false),
        note: Some("formal perfect participle, common in exam reading"),
    },
    CorpusEntry {
        id: "E03",
        source: "So quickly did the model respond that nobody noticed the latency.",
        translation: "모델이 너무나 빨리 응답해서 아무도 지연을 눈치채지 못했습니다.",
        word: None,
        category: "exam",
        expected_lemma: None,
        expected_pos: None,
        expected_structure_rubric: Some("so...that inversion"),
        expected_grammar: Some("so...that with fronted adverb and inversion"),
        expected_idiom: Some(false),
        note: Some("emphatic inversion pattern, exam favourite"),
    },
    // ---- F. Edge cases (3) ----
    CorpusEntry {
        id: "F01",
        source: "vacuously",
        translation: "공허하게",
        word: Some("vacuously"),
        category: "edge",
        expected_lemma: Some("vacuously"),
        expected_pos: Some("부사"),
        expected_structure_rubric: None,
        expected_grammar: Some("single adverb, no structure"),
        expected_idiom: Some(false),
        note: Some("single word: structure must be empty, lemma is the word itself"),
    },
    CorpusEntry {
        id: "F02",
        source: "The Worklynx dashboard shows one row per session.",
        translation: "Worklynx 대시보드는 세션당 한 줄을 표시합니다.",
        word: Some("Worklynx"),
        category: "edge",
        expected_lemma: Some("Worklynx"),
        expected_pos: Some("명사"),
        expected_structure_rubric: None,
        expected_grammar: Some("proper noun"),
        expected_idiom: Some(false),
        note: Some("proper noun: lemma should not change the form"),
    },
    CorpusEntry {
        id: "F03",
        source: "The timeout is 404ms, which exceeds the 200ms budget.",
        translation: "타임아웃은 404ms로, 200ms 예산을 초과합니다.",
        word: Some("timeout"),
        category: "edge",
        expected_lemma: Some("timeout"),
        expected_pos: Some("명사"),
        expected_structure_rubric: Some("relative clause"),
        expected_grammar: Some("numeric expression with relative clause"),
        expected_idiom: Some(false),
        note: Some("numbers and units in technical context"),
    },
];

/// Corpus revision identifier.
///
/// This is `canary-25-v1` with the source SHA traceable via git history.
/// The revision refers to the corpus definition, not the benchmark results.
pub const CORPUS_REVISION: &str = "canary-25-v1";

/// Number of entries in the corpus.
pub const CORPUS_SIZE: usize = 25;
