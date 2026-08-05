//! The contract between the domain and a translation engine.
//!
//! Same shape as [`crate::acquire`]: one trait, one result, one closed error
//! enum, and no knowledge of how the engine is reached. A cloud provider or a
//! second local runtime implements this and nothing above it changes.
//!
//! Two speeds, and only the first is defined here. Quick translation runs on
//! every settled hover and must stay inside a latency budget a reader will
//! tolerate. Deep analysis is a separate path with a separate model, a separate
//! cache, and no budget at all
//! (`docs/decisions/ADR-0005-two-speed-ai.md`).

use std::time::Duration;

/// What to translate, and into what.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuickRequest {
    /// The sentence around the pointer. Always sent, even when the user is
    /// pointing at one word: a bare word makes a model guess, and it guesses
    /// confidently. `gift` is a present in English and a poison in German.
    pub source_text: String,
    /// The word under the pointer, when there is one.
    pub focused_word: Option<String>,
    /// The language the user reads in.
    pub target_language: String,
}

impl QuickRequest {
    pub fn new(source_text: impl Into<String>, target_language: impl Into<String>) -> Self {
        Self {
            source_text: source_text.into(),
            focused_word: None,
            target_language: target_language.into(),
        }
    }

    pub fn with_focus(mut self, word: impl Into<String>) -> Self {
        self.focused_word = Some(word.into());
        self
    }

    /// Whether this is worth sending to an engine at all.
    ///
    /// Cheap refusals, made here rather than in an adapter so every engine
    /// inherits them: an empty string, and text with no letters in it. A
    /// pointer resting on `{ }` or `42` should cost nothing.
    pub fn is_worth_translating(&self) -> bool {
        let trimmed = self.source_text.trim();
        !trimmed.is_empty() && trimmed.chars().any(char::is_alphabetic)
    }
}

/// How many tokens an answer to this much text is allowed.
///
/// A fixed budget is wrong in both directions. 256 tokens for the word `OK`
/// let a 3B model return forty-one characters of explanation, which on a card
/// is noise the reader has to filter; and a budget large enough for a
/// paragraph is paid for in latency on every short hover.
///
/// So it scales with the question. Generously, because a translation cut off
/// mid-sentence is worse than a wordy one: three tokens per source character
/// is far more than any language pair needs.
///
/// **It does not make a small model terse**, and it was measured rather than
/// assumed: at 32 tokens `OK` still came back as 43 characters against 41 at
/// 256. What it does is bound the worst case and the latency that goes with
/// it. A short hover no longer pays for a budget it cannot use.
///
/// The floor exists because a very short source still needs room for a
/// complete sentence in a language that may express it at length.
pub fn token_budget(source_chars: usize) -> u32 {
    const FLOOR: u32 = 32;
    const CEILING: u32 = 256;
    let scaled = source_chars.saturating_mul(3).saturating_add(24);
    u32::try_from(scaled)
        .unwrap_or(CEILING)
        .clamp(FLOOR, CEILING)
}

/// Names that mean a model cannot produce prose at all.
///
/// Matched as substrings, case-insensitively. Short on purpose: it names what
/// is definitely wrong rather than enumerating what is right, because the
/// second list goes stale every month and this one does not.
///
/// **Code models are not on it, and were.** Excluding them was an assumption
/// that sounded obviously correct, and it was measured on 2026-08-05: over
/// three sentences `qwen2.5-coder:7b` was right every time where `qwen2.5:3b`
/// rendered "pointer" as 점퍼 and "output" as 출제. Size beat specialisation, and
/// a rule written from a plausible story would have chosen the worse model.
/// See `docs/benchmarks/2026-08-05-model-comparison.md`.
const NOT_FOR_PROSE: [&str; 5] = ["embed", "bge", "nomic", "whisper", "clip"];

/// Preferred translation models, best first, matched as a name prefix.
const PREFERRED: [&str; 3] = ["translategemma", "gemma3", "qwen2.5"];

/// Above this, a hover costs more than it is worth.
///
/// Larger was better up to a point, and the point is latency: 7B answered 200ms
/// slower than 3B and much better. Nothing measured says a 30B would be worth
/// the seconds it takes on a card the reader is already waiting for.
const BIGGEST_WORTH_LOADING: f32 = 9.0;

/// Parameter count in billions, from the tag, when the name states one.
///
/// `qwen2.5:3b`, `llama3.1:8b`, `qwen2.5-coder:7b`, and `gemma4:e4b`, whose `e`
/// means something else entirely but whose 4 is still the number to compare on.
///
/// `None` when the name says nothing, which is not the same as small: an
/// unlabelled model ranks below a labelled one rather than being assumed tiny.
pub fn parameter_billions(name: &str) -> Option<f32> {
    let lower = name.to_lowercase();
    let tag = lower.rsplit([':', '-']).next()?;
    if !tag.ends_with('b') {
        return None;
    }
    let digits: String = tag
        .chars()
        .skip_while(|character| !character.is_ascii_digit())
        .take_while(|character| character.is_ascii_digit() || *character == '.')
        .collect();
    if digits.is_empty() {
        return None;
    }
    digits.parse().ok()
}

/// Choose a model to translate with, from what is installed.
///
/// `wanted` is `Some` only when a person named a model, and then it wins
/// whenever it is installed. `None` means nobody chose, and the rules below
/// decide.
///
/// The distinction is load-bearing. While the fallback name was passed here as
/// though it were a choice, a machine that happened to have that exact model
/// installed short-circuited every rule, and the better model sitting beside it
/// was never considered.
///
/// **A code model is never chosen automatically.** The previous version matched
/// the prefix `qwen2.5`, which also matches `qwen2.5-coder:7b`, so a machine
/// with the coder model and nothing else would have translated prose with a
/// model trained to complete functions. It can still be asked for by name.
pub fn choose_model(wanted: Option<&str>, installed: &[String]) -> Option<String> {
    if let Some(wanted) = wanted {
        if installed.iter().any(|name| name == wanted) {
            return Some(wanted.to_owned());
        }
    }

    let usable: Vec<&String> = installed
        .iter()
        .filter(|name| {
            let lower = name.to_lowercase();
            !NOT_FOR_PROSE.iter().any(|bad| lower.contains(bad))
        })
        .collect();

    // The largest of a named family, not the first one listed. The families
    // are the ones actually measured; within a family, size is what the
    // measurement rewarded.
    let largest = |candidates: Vec<&String>| -> Option<String> {
        candidates
            .iter()
            .filter_map(|name| {
                parameter_billions(name)
                    .filter(|size| *size <= BIGGEST_WORTH_LOADING)
                    .map(|size| (size, (*name).clone()))
            })
            .max_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, name)| name)
            // A family whose members state no size at all still beats an
            // unmeasured stranger.
            .or_else(|| candidates.first().map(|name| (*name).clone()))
    };

    let chosen = PREFERRED
        .iter()
        .find_map(|prefix| {
            let family: Vec<&String> = usable
                .iter()
                .filter(|name| name.starts_with(prefix))
                .copied()
                .collect();
            (!family.is_empty()).then(|| largest(family)).flatten()
        })
        // Then the largest that is still worth waiting for. Larger was better
        // where it was measured, and the ceiling is what stops that reasoning
        // running away to the biggest model on the machine.
        .or_else(|| {
            usable
                .iter()
                .filter_map(|name| {
                    parameter_billions(name)
                        .filter(|size| *size <= BIGGEST_WORTH_LOADING)
                        .map(|size| (size, (*name).clone()))
                })
                .max_by(|a, b| a.0.total_cmp(&b.0))
                .map(|(_, name)| name)
        })
        .or_else(|| usable.first().map(|name| (*name).clone()));

    // Everything installed is a code or embedding model. Better to try one than
    // to refuse: the card names the model that answered, so a bad translation
    // is legible as one, and no translation is not.
    chosen.or_else(|| installed.first().cloned())
}

/// A finished quick translation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuickTranslation {
    pub text: String,
    /// Which engine answered, so the card can show it. The user is always able
    /// to tell whether what they are reading left the machine
    /// (`docs/privacy-model.md` §1).
    pub engine: EngineId,
    pub model: String,
    /// How long the engine took, the first time it was asked.
    ///
    /// Kept as measured even when the answer came back from the cache, because
    /// zero is not a measurement. The cache used to overwrite this with `0` and
    /// the card printed `0ms`, which reads as a broken clock rather than as a
    /// fast answer.
    pub latency_ms: u128,
    /// Whether this came from the cache rather than the engine.
    ///
    /// A separate flag rather than a magic latency: the card has to say
    /// something different, and "zero milliseconds" is not a different thing to
    /// say, it is a wrong one.
    pub cached: bool,
}

/// Which engine produced an answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineId {
    /// An Ollama on this machine. The default, and the only one that needs no
    /// grant.
    LocalOllama,
    /// Anything reached over the network. Requires an explicit per-provider
    /// grant, and the card says so.
    External,
}

impl EngineId {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LocalOllama => "local",
            Self::External => "external",
        }
    }

    /// Whether text sent to this engine leaves the machine.
    pub const fn leaves_the_machine(self) -> bool {
        matches!(self, Self::External)
    }
}

/// Every way a translation can fail to arrive.
///
/// Closed and typed for the same reason [`crate::acquire::AcquireError`] is:
/// "it did not work" is not a measurement. A user with no Ollama installed and
/// a user whose model is missing need different words on screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TranslateError {
    /// No engine answered at all. Usually Ollama is not running.
    Unavailable,
    /// The engine is there and the model is not.
    ModelMissing(String),
    /// The engine did not finish inside the budget.
    Timeout,
    /// The engine answered with something that was not a translation.
    Malformed,
    /// The pointer moved before this finished.
    Stale,
    /// Nothing worth translating: empty, or no letters in it.
    NotWorthTranslating,
}

impl TranslateError {
    pub fn reason(&self) -> &'static str {
        match self {
            Self::Unavailable => "unavailable",
            Self::ModelMissing(_) => "model-missing",
            Self::Timeout => "timeout",
            Self::Malformed => "malformed",
            Self::Stale => "stale",
            Self::NotWorthTranslating => "not-worth-translating",
        }
    }

    /// What to put in front of a user, in Korean, as product copy.
    ///
    /// Here rather than in the overlay because the mapping from a failure to
    /// an explanation is a product decision, and an adapter should not be able
    /// to invent its own wording.
    pub fn user_message(&self) -> String {
        match self {
            Self::Unavailable => "Ollama에 연결할 수 없습니다".to_owned(),
            Self::ModelMissing(model) => format!("모델 {model}이(가) 설치되어 있지 않습니다"),
            Self::Timeout => "번역이 시간 안에 끝나지 않았습니다".to_owned(),
            Self::Malformed => "번역 결과를 이해할 수 없습니다".to_owned(),
            Self::Stale => "취소되었습니다".to_owned(),
            Self::NotWorthTranslating => "번역할 내용이 없습니다".to_owned(),
        }
    }

    /// What the user should do about it, when there is anything to do.
    ///
    /// [`Self::user_message`] says what went wrong. That is enough for a
    /// reader who already has the product working and hit a hiccup, and it is
    /// a dead end for the person this matters most to: someone who installed
    /// Lens, has no Ollama, hovers over a sentence, and is told that Ollama
    /// cannot be reached. They now know a name they have never heard and no
    /// next step.
    ///
    /// `None` for the two states that are not the user's problem to fix. A
    /// cancelled hover with advice attached would be noise on a card that
    /// should barely register.
    pub fn remedy(&self) -> Option<String> {
        match self {
            Self::Unavailable => {
                Some("ollama.com 에서 Ollama를 설치한 뒤 실행해 주세요".to_owned())
            }
            // Names the command, because the alternative is a user searching
            // for how to install a model they did not choose.
            Self::ModelMissing(model) => Some(format!("터미널에서 ollama pull {model}")),
            Self::Timeout => Some("더 작은 모델을 쓰거나 잠시 후 다시 시도해 주세요".to_owned()),
            Self::Malformed => Some("다른 모델로 바꿔 보세요".to_owned()),
            Self::Stale | Self::NotWorthTranslating => None,
        }
    }
}

impl std::fmt::Display for TranslateError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.reason())
    }
}

impl std::error::Error for TranslateError {}

/// Turns text into the user's language.
pub trait Translator {
    /// The path a hover takes. Must respect a budget: a reader waiting on this
    /// has already stopped reading.
    fn quick(&self, request: &QuickRequest) -> Result<QuickTranslation, TranslateError>;

    /// The same, reporting the answer as it arrives.
    ///
    /// `on_partial` is called with the text so far. It is **throttled by the
    /// implementation**, not called per token: a short translation redrawn on
    /// every token shimmers, which reads as instability rather than as speed.
    ///
    /// The default forwards to [`Self::quick`] and reports once, so an engine
    /// that cannot stream is a slower engine rather than a broken one.
    fn quick_streaming(
        &self,
        request: &QuickRequest,
        on_partial: &mut dyn FnMut(&str),
    ) -> Result<QuickTranslation, TranslateError> {
        let answer = self.quick(request)?;
        on_partial(&answer.text);
        Ok(answer)
    }
}

/// How often a partial answer may redraw the card.
///
/// Slow enough that text does not shimmer, fast enough that it reads as
/// arriving rather than as three discrete jumps.
pub const PARTIAL_REDRAW_INTERVAL: Duration = Duration::from_millis(120);

/// How long a hover may wait before the answer is worthless.
///
/// Generous against the 700ms target in the research brief, because the cost of
/// exceeding it is a card that arrives late rather than a wrong answer, and a
/// cold model on a slow machine is common. Mori measured a 9.6GB model taking
/// 35 seconds to load cold; that case must fail rather than hang, and it must
/// not be mistaken for the model being missing.
pub const QUICK_TIMEOUT: Duration = Duration::from_secs(20);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_with_no_letters_is_not_worth_a_model_call() {
        // A pointer resting on punctuation or a number should cost nothing.
        for text in ["", "   ", "{ }", "42", "-->", "1.0.0"] {
            assert!(
                !QuickRequest::new(text, "ko").is_worth_translating(),
                "{text:?} should not be translated",
            );
        }
    }

    #[test]
    fn ordinary_text_is_worth_translating() {
        for text in [
            "produce",
            "The suite did not produce anything.",
            "출력",
            "日本語",
        ] {
            assert!(
                QuickRequest::new(text, "ko").is_worth_translating(),
                "{text:?} should be translated",
            );
        }
    }

    #[test]
    fn a_local_engine_does_not_send_text_off_the_machine() {
        assert!(!EngineId::LocalOllama.leaves_the_machine());
        assert!(EngineId::External.leaves_the_machine());
    }

    /// The two states a first-time user meets are the two that must carry a
    /// next step. Everything else may.
    #[test]
    fn the_failures_a_new_user_meets_say_what_to_do_about_them() {
        let unreachable = TranslateError::Unavailable.remedy().expect("a remedy");
        assert!(
            unreachable.contains("ollama.com"),
            "a user told only that Ollama is unreachable has nowhere to go: {unreachable}",
        );

        let missing = TranslateError::ModelMissing("qwen2.5:3b".to_owned())
            .remedy()
            .expect("a remedy");
        assert!(
            missing.contains("ollama pull qwen2.5:3b"),
            "the remedy must be the command, not a description of it: {missing}",
        );
    }

    /// A cancelled hover is not a problem the user caused or can fix, and
    /// advice on that card would be noise.
    #[test]
    fn a_cancelled_hover_offers_no_advice() {
        assert_eq!(TranslateError::Stale.remedy(), None);
        assert_eq!(TranslateError::NotWorthTranslating.remedy(), None);
    }

    #[test]
    fn a_short_label_asks_for_far_less_than_a_paragraph() {
        assert_eq!(token_budget(2), 32);
        assert!(token_budget("Sign in".chars().count()) < 64);
    }

    #[test]
    fn a_sentence_is_given_room_to_finish() {
        // A translation cut off mid-sentence is worse than a wordy one, so the
        // budget stays generous for anything sentence-sized.
        let sentence = "The output files came back empty, so the full test suite                         produced nothing.";
        assert!(token_budget(sentence.chars().count()) >= 200);
    }

    #[test]
    fn a_paragraph_cannot_ask_for_an_unbounded_reply() {
        assert_eq!(token_budget(100_000), 256);
        assert_eq!(token_budget(usize::MAX), 256);
    }

    fn models(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| (*name).to_string()).collect()
    }

    #[test]
    fn a_model_the_user_named_wins_over_every_preference() {
        let installed = models(&["gemma3:4b", "qwen2.5:3b"]);
        assert_eq!(
            choose_model(Some("qwen2.5:3b"), &installed).as_deref(),
            Some("qwen2.5:3b"),
        );
    }

    /// A measured family wins over an unmeasured one whatever the sizes.
    /// What the 2026-08-05 comparison actually asked for. Both are the same
    /// family; the 7B was right on all three sentences and the 3B rendered
    /// "pointer" as 점퍼, for 200ms.
    #[test]
    fn the_larger_member_of_a_family_is_chosen() {
        let installed = models(&["qwen2.5:3b", "qwen2.5-coder:7b"]);
        assert_eq!(
            choose_model(None, &installed).as_deref(),
            Some("qwen2.5-coder:7b"),
        );
    }

    #[test]
    fn a_measured_family_is_preferred_over_a_bigger_stranger() {
        let installed = models(&["some-stranger:8b", "gemma3:4b"]);
        assert_eq!(choose_model(None, &installed).as_deref(), Some("gemma3:4b"),);
    }

    /// Among models nobody has measured, larger won where it was measured.
    #[test]
    fn the_largest_worthwhile_stranger_is_chosen() {
        let installed = models(&["stranger:3b", "stranger:8b", "stranger:1b"]);
        assert_eq!(
            choose_model(None, &installed).as_deref(),
            Some("stranger:8b"),
        );
    }

    /// And not the biggest thing on the machine. A hover is not the place to
    /// load seventy billion parameters.
    #[test]
    fn a_model_too_large_for_a_hover_is_left_alone() {
        let installed = models(&["stranger:70b", "stranger:7b"]);
        assert_eq!(
            choose_model(None, &installed).as_deref(),
            Some("stranger:7b"),
        );
    }

    #[test]
    fn a_parameter_count_is_read_from_the_tag_when_there_is_one() {
        assert_eq!(parameter_billions("qwen2.5:3b"), Some(3.0));
        assert_eq!(parameter_billions("llama3.1:8b"), Some(8.0));
        // The `e` means something else; the 4 is still what to compare on.
        assert_eq!(parameter_billions("gemma4:e4b"), Some(4.0));
        assert_eq!(parameter_billions("qwen2.5-coder:7b"), Some(7.0));
        // Silence is not smallness.
        assert_eq!(parameter_billions("qwen2.5"), None);
        assert_eq!(parameter_billions("nomic-embed-text"), None);
    }

    #[test]
    fn an_embedding_model_is_not_a_translator_either() {
        let installed = models(&["nomic-embed-text", "qwen2.5:3b"]);
        assert_eq!(
            choose_model(None, &installed).as_deref(),
            Some("qwen2.5:3b"),
        );
    }

    /// The measurement that changed this rule, as a test.
    ///
    /// On 2026-08-05 the coder model translated three sentences correctly where
    /// the 3B general model rendered "pointer" as 점퍼. Excluding code models by
    /// name had been an assumption, and this is the machine it was wrong on.
    #[test]
    fn a_code_model_is_not_excluded_because_of_its_name() {
        let installed = models(&["qwen2.5-coder:7b"]);
        assert_eq!(
            choose_model(None, &installed).as_deref(),
            Some("qwen2.5-coder:7b"),
        );
    }

    /// Embedding models really cannot answer, so those stay excluded. With
    /// nothing else installed, trying beats refusing: the card names the model,
    /// so a bad answer is legible as one and no answer is not.
    #[test]
    fn a_machine_with_only_an_embedding_model_still_gets_a_try() {
        let installed = models(&["nomic-embed-text"]);
        assert_eq!(
            choose_model(None, &installed).as_deref(),
            Some("nomic-embed-text"),
        );
    }

    #[test]
    fn nothing_installed_means_nothing_chosen() {
        assert_eq!(choose_model(Some("qwen2.5:3b"), &[]), None);
    }

    #[test]
    fn an_unlisted_model_is_used_when_it_is_all_there_is() {
        let installed = models(&["some-new-model:8b"]);
        assert_eq!(
            choose_model(None, &installed).as_deref(),
            Some("some-new-model:8b"),
        );
    }

    #[test]
    fn a_missing_model_says_which_one() {
        let error = TranslateError::ModelMissing("qwen2.5:3b".to_owned());
        assert!(
            error.user_message().contains("qwen2.5:3b"),
            "the user cannot act on a message that does not name the model",
        );
    }
}
