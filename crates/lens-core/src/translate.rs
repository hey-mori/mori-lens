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
    ///
    /// Measured on 2026-08-18, six ambiguous words on `qwen2.5:7b` sent inside
    /// their sentence and alone: the lone word gave the reading its sentence
    /// forces **1 time in 6**, and saved about 540ms doing it. `bank` came back
    /// as a financial bank, `spring` as the season, `lead` as a leader on one
    /// run and as initiative on the next. See
    /// `docs/benchmarks/2026-08-18-a-word-without-its-sentence.md`, and
    /// `live_tests::measure_what_a_word_scoped_request_loses` for the check
    /// that re-measures it rather than remembering this paragraph.
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

    /// Name the word the reader is looking at, when there is one.
    ///
    /// **A focus that is the whole sentence is not a focus**, and is dropped.
    /// Measured from a reader's log on 2026-08-07:
    ///
    /// ```text
    /// ocr   94 chars on the line, focus 94 chars
    /// ```
    ///
    /// The recogniser had merged an entire line into one "word", so the prompt
    /// said *the reader is looking at the word "…"* and quoted the sentence
    /// back at the model. That is not a hint, it is the question restated, and
    /// it costs tokens on a path where the reader is waiting.
    ///
    /// Compared on whitespace-normalised text, because the sentence arrives
    /// with line-wrapping artefacts and the word does not.
    pub fn with_focus(mut self, word: impl Into<String>) -> Self {
        let word = word.into();
        let same = |text: &str| text.split_whitespace().collect::<Vec<_>>().join(" ");
        self.focused_word =
            (!word.trim().is_empty() && same(&word) != same(&self.source_text)).then_some(word);
        self
    }

    /// Whether this is worth sending to an engine at all.
    ///
    /// Cheap refusals, made here rather than in an adapter so every engine
    /// inherits them: an empty string, and text with no letters in it. A
    /// pointer resting on `{ }` or `42` should cost nothing.
    pub fn is_worth_translating(&self) -> bool {
        let trimmed = self.source_text.trim();
        !trimmed.is_empty()
            && trimmed.chars().any(char::is_alphabetic)
            && !mostly_identifiers(trimmed)
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

/// How a fence marker opens, which is the only part of it that is fixed.
pub const FENCE_OPENING: &str = "#LENS-";

/// The least of a marker that has to be present before it is treated as one.
///
/// The opening plus eight hexadecimal digits. Shorter than that and a `#LENS-`
/// arriving off somebody's screen, out of this repository's own source, would
/// be cut out of their translation. Longer is unnecessary: eight digits of a
/// hash generated this second are not going to be typed by accident.
const LEAST_TO_BE_THE_FENCE: usize = FENCE_OPENING.len() + 8;

/// Take the frame back off a picture a model handed over with it still on.
///
/// The quick prompt says *no markers* and nothing checked. Measured 2026-08-18,
/// the source `empty` came back as
///
/// ```text
/// #LENS-5a23bfc686af5a74#
/// 공백
/// #LENS-5a23bfc686af5a7
/// ```
///
/// where `공백` is the correct answer and the rest is the frame, with the
/// closing marker cut short because the token budget ran out inside it. So the
/// answer was right and what reached the card was not, and refusing it would
/// throw away a good translation to punish a formatting mistake.
///
/// **A partial marker is still a marker.** The truncated line above shares
/// fifteen characters with the fence and no closing `#`, so anything matching
/// only the whole marker leaves it on the card.
///
/// This cuts prefixes of **this request's own fence** rather than anything
/// beginning `#LENS-`, so a reader hovering over this repository's own source
/// keeps their text.
pub fn without_the_fence(text: &str, fence: &str) -> String {
    if fence.len() < LEAST_TO_BE_THE_FENCE {
        return text.trim().to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(FENCE_OPENING) {
        out.push_str(&rest[..at]);
        let tail = &rest[at..];
        // The longest prefix of the fence that is actually here. The fence is
        // ASCII by construction, so slicing it by byte is slicing it by char.
        let matched = (LEAST_TO_BE_THE_FENCE..=fence.len())
            .filter(|&n| tail.starts_with(&fence[..n]))
            .max();
        match matched {
            Some(n) => rest = &tail[n..],
            // A `#LENS-` that is not this fence. Somebody else's text, kept.
            None => {
                out.push_str(FENCE_OPENING);
                rest = &tail[FENCE_OPENING.len()..];
            }
        }
    }
    out.push_str(rest);
    out.trim().to_owned()
}

/// Whether an answer is just the question again.
///
/// A user hovered a long sentence and the card showed the English original
/// with, underneath it in bold, the same English word for word. Every check in
/// this crate passed: the reply was non-empty, well-formed, the right length,
/// and no error field anywhere. The one thing wrong with it was the only thing
/// that mattered, and nothing was looking.
///
/// Wording in the system prompt reduced it to nothing measurable on one model
/// (`lens_translate::system_prompt`), but a prompt is a request and this is a
/// guarantee. A different model, or the next release of the same one, gets to
/// break the request; this catches it either way.
///
/// # What counts
///
/// The proportion of the source's words that survive verbatim in the answer.
/// Not equality: a model echoes with a stray full stop or a dropped article and
/// that is still an echo.
///
/// Two things keep it from firing on a real translation:
///
/// - **Short sources are exempt.** Translating `commit` into Korean may well
///   leave `commit` standing, and one word out of one is a ratio of 1.0. Below
///   [`ECHO_FLOOR`] words there is not enough evidence to accuse anyone.
/// - **The bar is high.** A Korean translation of an identifier-heavy sentence
///   legitimately keeps every backticked name, so some overlap is normal. What
///   is not normal is nearly all of it.
pub fn reads_as_the_source(source: &str, answer: &str) -> bool {
    /// Fewest source words before this is willing to judge.
    const ECHO_FLOOR: usize = 8;
    /// How much of the source has to survive before it is an echo, not a
    /// translation that kept the names in it.
    const ECHO_RATIO: f32 = 0.8;
    /// How long an unbroken run of source has to be before that run alone is
    /// an echo, however much of the sentence was translated around it.
    ///
    /// Six words. Shorter runs are ordinary: a title, a quoted phrase, a
    /// library name and its arguments all survive a good translation intact.
    /// Six consecutive words of untranslated prose do not.
    const RUN_FLOOR: usize = 6;

    let source_words = comparable_words(source);
    if source_words.len() < ECHO_FLOOR {
        return false;
    }
    let answer_words = comparable_words(answer);

    let kept = source_words
        .iter()
        .filter(|word| answer_words.contains(word))
        .count();
    if kept as f32 / source_words.len() as f32 >= ECHO_RATIO {
        return true;
    }

    // **The half-translated answer**, which the ratio above lets through and
    // which a reader met on a card: the first clause in Korean and the rest of
    // the sentence in English, verbatim, with nothing to say so. Counting
    // scattered survivors cannot see it, because half a sentence is under any
    // threshold that does not also fire on a translation full of identifiers.
    // An unbroken run can.
    longest_run(&source_words, &answer_words) >= RUN_FLOOR
}

/// Words as they compare: lower-case, stripped of anything but letters,
/// digits and underscores, and with what is left of an empty word dropped.
pub(crate) fn comparable_words(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|word| {
            word.trim_matches(|c: char| !c.is_alphanumeric() && c != '_')
                .to_lowercase()
        })
        .filter(|word| !word.is_empty())
        .collect()
}

/// The longest run of `source` words appearing consecutively in `answer`.
///
/// Quadratic, over two sentences. A hover sends one sentence and the budget
/// caps the answer, so this is tens of words against tens of words and costs
/// less than the allocation above it.
fn longest_run(source: &[String], answer: &[String]) -> usize {
    let mut best = 0;
    for start in 0..source.len() {
        for begin in 0..answer.len() {
            let mut run = 0;
            while start + run < source.len()
                && begin + run < answer.len()
                && source[start + run] == answer[begin + run]
            {
                run += 1;
            }
            best = best.max(run);
        }
    }
    best
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

/// Model families measured to work for a hover, matched as a name prefix.
///
/// **Only families that have actually been run.** It began as
/// `["translategemma", "gemma3", "qwen2.5"]`, and the first two were there on
/// reputation: one sounds like a translation engine and the other sounds
/// capable.
///
/// `translategemma` was pulled and measured on 2026-08-05. It is not a
/// translation engine, it is a chat model with a translation-sounding name:
/// given bare text it spent 53 seconds writing English commentary, and with a
/// short instruction it answered in markdown with alternatives. Through this
/// crate's fenced prompt it behaves, and is **2.5 times slower** than the
/// family below, with one timeout in three sentences.
///
/// It was first in this list. A user who had it installed would have been
/// routed to it by a preference nobody had ever run.
///
/// `gemma3` is removed for the same reason in advance: never run here either.
/// A model nobody has measured is a guess rather than a preference, and
/// everything not named here is still chosen by size, which is a rule with
/// evidence behind it.
const PREFERRED: [&str; 1] = ["qwen2.5"];

/// Above this, a hover costs more than it is worth.
///
/// Larger was better up to a point, and the point is latency: 7B answered 200ms
/// slower than 3B and much better. Nothing measured says a 30B would be worth
/// the seconds it takes on a card the reader is already waiting for.
const BIGGEST_WORTH_LOADING: f32 = 9.0;

/// Specialisation suffixes that lose a tie against a plain model.
///
/// **Only a tie.** A code model is still chosen over a smaller general one, and
/// `a_code_model_is_not_excluded_because_of_its_name` is still the measurement
/// that says so. This decides the case that measurement never covered: two
/// models of the same family and the same size, one specialised.
///
/// Measured 2026-08-06, `qwen2.5:7b` against `qwen2.5-coder:7b`, five
/// sentences. The plain model was correct on the two the coder failed worst:
/// it rendered "worse than no card" the right way round, and it did not invent
/// a second sentence that was not in the source. Warm medians were 2,644ms and
/// 2,462ms, which is noise, and the coder's spread reached 4,882ms against the
/// plain model's 2,931ms. See `docs/benchmarks/2026-08-06-model-at-one-size.md`.
///
/// Without this the choice was **whichever Ollama happened to list last**, on
/// any machine with both installed, since equal sizes make the size comparison
/// a tie. A product whose model depends on a listing order is a product where
/// nobody can explain the answer.
const SPECIALISED: [&str; 4] = ["-coder", "-code", "-math", "-vision"];

/// Whether a name carries a specialisation suffix. See [`SPECIALISED`].
fn is_specialised(name: &str) -> bool {
    let lower = name.to_lowercase();
    SPECIALISED.iter().any(|suffix| lower.contains(suffix))
}

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
/// **A named model that is not installed does not fall through to the rules
/// here**, and the caller is what stops it: `OllamaTranslator::resolve_model`
/// answers `ModelMissing` before asking. It used to fall through, and a reader
/// who named a model they had not pulled got a different one with nothing said
/// about it. This function keeps the fall-through because it is the right
/// behaviour for the `None` case and because a pure function is the wrong
/// place to decide whose promise was broken.
///
/// The distinction is load-bearing. While the fallback name was passed here as
/// though it were a choice, a machine that happened to have that exact model
/// installed short-circuited every rule, and the better model sitting beside it
/// was never considered.
///
/// **A code model can be chosen automatically, and that is deliberate.** It
/// was excluded once, on the reasoning that a model trained to complete
/// functions should not translate prose. That reasoning was measured on
/// 2026-08-05 and lost: `qwen2.5-coder:7b` was right where `qwen2.5:3b`
/// rendered "pointer" as 점퍼. `a_code_model_is_not_excluded_because_of_its_name`
/// is that measurement.
///
/// This paragraph said the opposite until 2026-08-06, describing the excluded
/// behaviour as current, and it is worth saying why that mattered rather than
/// just correcting it. A reader asked why the translations were poor. The model
/// answering was the coder model, chosen by this function, while the comment
/// above the function said that could not happen. A stale comment about a rule
/// is not a small thing when somebody is trying to work out where an answer
/// came from.
///
/// What the 2026-08-05 measurement establishes is narrower than it was read as:
/// 7B beat 3B. It compared a code model against a general model **of a
/// different size**, so it says nothing about specialisation at a fixed size.
/// See `docs/benchmarks/2026-08-06-translation-quality.md`.
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
            // Size first, then the plain model over the specialised one. The
            // tie-break is second because a bigger code model still beats a
            // smaller general one, which is what 2026-08-05 measured.
            .max_by(|a, b| {
                a.0.total_cmp(&b.0)
                    .then_with(|| is_specialised(&b.1).cmp(&is_specialised(&a.1)))
            })
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
    /// Whether part of the answer was removed before the reader saw it.
    ///
    /// **The one case where a card shows less than the engine said.** A model
    /// that switches language mid-sentence produces an answer whose first half
    /// is a correct translation and whose second half is in a third language,
    /// and [`crate::language::without_the_third_language`] keeps the first half.
    ///
    /// That is the right thing to keep. What was missing is that a reader was
    /// then shown a sentence ending mid-phrase with a footer saying it took 3.9
    /// seconds, which reads as a finished answer and is not one. The comment on
    /// the rule that does the cutting already said so: *better than half of one
    /// presented as a translation*, of the case where nothing is left.
    pub part_was_removed: bool,
    /// Whether an identifier the prompt named did not come back.
    ///
    /// Measured 2026-08-18: `LedgerDataSheetService` is found by
    /// [`identifiers_in`], listed in the request so the model copies it, and
    /// transliterated to 레저데이터시트서비스 anyway, alone and inside an
    /// ordinary sentence. A wording added to the prompt to insist scored
    /// exactly the same, three samples running.
    ///
    /// So the card says so instead. The translation around the name is usually
    /// right and throwing it away would cost the reader a good sentence; what
    /// they cannot afford is copying a name that does not exist.
    pub name_changed: bool,
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

/// Which engine an endpoint address describes.
///
/// The one place that decides whether text sent to an address leaves the
/// machine, so the card's 로컬 or 외부 is derived from the address rather than
/// asserted next to it. It was asserted: the Ollama adapter stamped
/// `LocalOllama` on every answer regardless of where it had sent the request,
/// which was true only because the address was a constant.
///
/// Loopback is `localhost`, anything in `127.0.0.0/8`, and `::1`. Everything
/// else is external, including a name that resolves to a loopback address:
/// this cannot resolve names and guessing from a string is how a promise gets
/// broken quietly.
pub fn engine_for(endpoint: &str) -> EngineId {
    let host = endpoint.trim();
    // `[::1]:11434`, the bracketed form, before splitting on a colon that the
    // address itself is full of.
    let host = if let Some(rest) = host.strip_prefix('[') {
        rest.split(']').next().unwrap_or_default()
    } else {
        host.rsplit_once(':').map_or(host, |(host, _)| host)
    };

    let loopback = host.eq_ignore_ascii_case("localhost")
        || host == "::1"
        || host
            .split_once('.')
            .is_some_and(|(first, _)| first == "127" && host.split('.').count() == 4);
    if loopback {
        EngineId::LocalOllama
    } else {
        EngineId::External
    }
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
    /// The engine answered with the question. See [`reads_as_the_source`].
    NotTranslated,
    /// The engine answered with the question, and it was right to: the reader
    /// can already read a good part of the source.
    ///
    /// Separate from [`Self::NotTranslated`] because the two need opposite
    /// words on a card. A reader met the wrong one: a line of Korean prose
    /// carrying English identifiers came back unchanged, correctly, and Lens
    /// told them the model had failed and to try another one.
    AlreadyReadable,
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
            Self::NotTranslated => "not-translated",
            Self::AlreadyReadable => "already-readable",
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
            // Deliberately says what happened rather than blaming the reader.
            // The card cannot show the answer, because the answer is the
            // sentence they are already looking at.
            Self::NotTranslated => "모델이 번역하지 않고 원문을 그대로 돌려줬습니다".to_owned(),
            Self::AlreadyReadable => "이미 읽을 수 있는 언어입니다".to_owned(),
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
            // The same advice, and it is the right advice: this failure is a
            // property of the model, and the user has others installed.
            Self::NotTranslated => Some("다른 모델로 바꿔 보세요".to_owned()),
            // Nothing to fix. The reader can read it.
            Self::AlreadyReadable => None,
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
        // One line. A line continuation inside a string literal comes back
        // from rustfmt rejoined with its indentation, and this very sentence
        // carried twenty-five spaces in the middle of it until 2026-08-06.
        let sentence =
            "The output files came back empty, so the full test suite produced nothing at all.";
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

    /// A model whose name carries no size is reachable, and does not win.
    ///
    /// Filed as a bug on 2026-08-18 and it is not one, which is worth a test
    /// rather than a closed issue. `parameter_billions` returns nothing for
    /// `translategemma:latest`, so it is dropped from both size rankings. The
    /// last fallback still reaches it, so the behaviour is: **loses to anything
    /// measured, wins when it is all there is.**
    ///
    /// What is real about that report is a different thing, and it is in
    /// `PREFERRED`'s own comment rather than here: `translategemma` is out of
    /// that list because it was run on 2026-08-05 and lost.
    #[test]
    fn a_name_with_no_size_loses_but_is_still_reachable() {
        let alone = vec!["translategemma:latest".to_owned()];
        assert_eq!(
            choose_model(None, &alone).as_deref(),
            Some("translategemma:latest"),
            "the only installed model must be chosen even with no size in its name",
        );

        let beside_a_measured_family =
            vec!["qwen2.5:7b".to_owned(), "translategemma:latest".to_owned()];
        assert_eq!(
            choose_model(None, &beside_a_measured_family).as_deref(),
            Some("qwen2.5:7b"),
            "a measured family outranks a name nobody can size",
        );

        // And naming it still works, which is the reader's way past all of this.
        assert_eq!(
            choose_model(Some("translategemma:latest"), &beside_a_measured_family).as_deref(),
            Some("translategemma:latest"),
            "a model named in settings is used whatever the ranking thinks",
        );
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
        let installed = models(&["some-stranger:8b", "qwen2.5:3b"]);
        assert_eq!(
            choose_model(None, &installed).as_deref(),
            Some("qwen2.5:3b")
        );
    }

    /// The one this list was reordered for. `translategemma` was first here on
    /// the strength of its name; measured, it is 2.5 times slower than the
    /// family below it and timed out on one sentence in three.
    #[test]
    fn a_model_named_after_translation_is_not_preferred_for_being_named_that() {
        let installed = models(&["translategemma:latest", "qwen2.5-coder:7b"]);
        assert_eq!(
            choose_model(None, &installed).as_deref(),
            Some("qwen2.5-coder:7b"),
        );
    }

    /// And it is still used when it is what the machine has. Slow beats
    /// nothing, and the card shows the latency.
    #[test]
    fn a_slow_model_is_still_better_than_no_translation() {
        let installed = models(&["translategemma:latest"]);
        assert_eq!(
            choose_model(None, &installed).as_deref(),
            Some("translategemma:latest"),
        );
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

    /// At the same size, the plain model wins.
    ///
    /// The case the 2026-08-05 measurement never covered, and the one this
    /// machine was actually in. Both are `qwen2.5`, both are 7B, so the size
    /// comparison is a tie and the winner was **whichever Ollama listed last**.
    ///
    /// Measured 2026-08-06 over five sentences: the plain model rendered
    /// "worse than no card" the right way round where the coder garbled it, and
    /// did not invent a second sentence where the coder did. Both orderings are
    /// asserted, because a tie-break that only works one way round is the bug
    /// it is fixing.
    #[test]
    fn at_the_same_size_the_plain_model_beats_the_specialised_one() {
        for order in [
            &["qwen2.5:7b", "qwen2.5-coder:7b"],
            &["qwen2.5-coder:7b", "qwen2.5:7b"],
        ] {
            let installed = models(order.as_slice());
            assert_eq!(
                choose_model(None, &installed).as_deref(),
                Some("qwen2.5:7b"),
                "listed as {order:?}",
            );
        }
    }

    /// And a bigger code model still beats a smaller general one, which is what
    /// the earlier measurement established and what the tie-break must not undo.
    #[test]
    fn a_bigger_code_model_still_beats_a_smaller_plain_one() {
        let installed = models(&["qwen2.5:3b", "qwen2.5-coder:7b"]);
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

#[cfg(test)]
mod echo_tests {
    use super::reads_as_the_source;

    const SOURCE: &str = "For the mechanics: I'd dump the two schemas and history table from the current database, create a fresh `orbbase` database owned by `orbbase_app`, then drop the old schemas after a grace period.";

    #[test]
    fn the_answer_that_was_the_question_is_caught() {
        assert!(reads_as_the_source(SOURCE, SOURCE));
    }

    #[test]
    fn an_echo_with_a_stray_edit_is_still_an_echo() {
        let nearly = SOURCE
            .replace("the current database", "the database")
            .to_owned()
            + " Thanks.";
        assert!(reads_as_the_source(SOURCE, &nearly));
    }

    #[test]
    fn a_korean_translation_that_keeps_the_identifiers_is_not_an_echo() {
        // What a good answer to that sentence looks like: every name kept,
        // every word around them replaced. This is the case the ratio has to
        // survive, and it is why the bar is 0.8 rather than something lower.
        let good = "구조적으로는 현재 데이터베이스에서 두 스키마와 이력 테이블을 덤프하고, `orbbase_app`이 소유하는 새 `orbbase` 데이터베이스를 만든 다음, 유예 기간이 지나면 예전 스키마를 삭제합니다.";
        assert!(!reads_as_the_source(SOURCE, good));
    }

    #[test]
    fn a_short_source_is_never_accused() {
        // One word in, the same word out, is an ordinary translation of a
        // loanword and not evidence of anything.
        assert!(!reads_as_the_source("commit", "commit"));
        assert!(!reads_as_the_source("git push", "git push"));
    }

    #[test]
    fn an_unrelated_answer_is_not_an_echo() {
        assert!(!reads_as_the_source(SOURCE, "완전히 다른 문장입니다."));
    }
}

#[cfg(test)]
mod endpoint_tests {
    use super::{engine_for, EngineId};

    #[test]
    fn the_default_and_its_neighbours_are_local() {
        for endpoint in [
            "127.0.0.1:11434",
            "127.0.0.1:1234",
            "localhost:11434",
            "LocalHost:11434",
            "127.1.2.3:11434",
            "[::1]:11434",
        ] {
            assert_eq!(
                engine_for(endpoint),
                EngineId::LocalOllama,
                "{endpoint} is on this machine",
            );
        }
    }

    #[test]
    fn anything_else_is_external() {
        for endpoint in [
            "192.168.1.10:11434",
            "10.0.0.5:11434",
            "ollama.example.com:11434",
            "0.0.0.0:11434",
            "1270.0.0.1:11434",
            "127x0.0.1:11434",
            "",
        ] {
            assert_eq!(
                engine_for(endpoint),
                EngineId::External,
                "{endpoint} is not this machine",
            );
        }
    }

    #[test]
    fn a_name_that_might_resolve_to_loopback_is_still_external() {
        // This cannot resolve names, and guessing from the string is how a
        // promise about where text goes gets broken quietly.
        assert_eq!(engine_for("my-loopback-host:11434"), EngineId::External);
        assert_eq!(
            engine_for("127.0.0.1.example.com:11434"),
            EngineId::External
        );
    }
}

#[cfg(test)]
mod half_translated_tests {
    use super::reads_as_the_source;

    /// The card a reader met: the first clause in Korean, the rest of the
    /// sentence in English, verbatim, with nothing to say so.
    #[test]
    fn a_translation_that_gives_up_halfway_is_caught() {
        let source = "A connection-check approach is better than an environment variable gate here, it is honest about what is needed and keeps the test green in environments with a database while avoiding false failures.";
        let half = "환경 변수 게이트보다 연결 확인 접근법이 더 나은 것 같습니다. it is honest about what is needed and keeps the test green in environments with a database while avoiding false failures.";
        assert!(reads_as_the_source(source, half));
    }

    /// And a whole one is not, however many names it keeps.
    #[test]
    fn a_translation_that_keeps_names_is_not_caught() {
        let source = "The build failed because cargo fmt --check found a difference in src/main.rs, so the pipeline stopped before the tests ran.";
        let good = "cargo fmt --check 가 src/main.rs 에서 차이를 찾아 빌드가 실패했고, 그래서 파이프라인은 테스트가 돌기 전에 멈췄습니다.";
        assert!(!reads_as_the_source(source, good));
    }

    /// A quoted phrase inside a good translation is not a run worth catching.
    ///
    /// This is what the six-word floor is protecting: a title, a quoted
    /// fragment or a library call survives a correct translation intact, and
    /// firing on those would refuse answers that are right.
    #[test]
    fn a_short_quoted_fragment_survives_without_being_an_echo() {
        let source = "The error message says connection refused by the server, which usually means the port is wrong rather than the host.";
        let good = "오류 메시지는 connection refused by the 라고 말하는데, 이는 보통 호스트가 아니라 포트가 잘못됐다는 뜻입니다.";
        assert!(!reads_as_the_source(source, good));
    }
}

/// Identifiers that went into a request and did not come out of the answer.
///
/// The prompt lists the identifiers it found so the model copies them rather
/// than applying a rule to them. Measured 2026-08-18, that is not enough:
/// `LedgerDataSheetService` is found, named, and transliterated anyway, three
/// runs of three, **alone and inside an ordinary sentence**. Naming it harder
/// changed nothing at all: a wording added to the system prompt scored exactly
/// the same 9 of 18 as the shipped one, three samples running.
///
/// So this checks the answer instead of asking the model again.
///
/// **Containment, not equality.** Korean attaches particles, so a surviving
/// `export_csv` arrives as `export_csv를` and a rule about whole words would
/// call that a loss. What is being looked for is the token's absence.
///
/// Only identifiers `identifiers_in` names are checked, because those are the
/// ones the model was told about. `windows-rs` is not among them, and adding a
/// hyphen to that rule would tell a model to keep `well-known` verbatim, which
/// is the same shape and an ordinary word.
pub fn identifiers_lost(source: &str, answer: &str) -> Vec<String> {
    identifiers_in(source)
        .into_iter()
        .filter(|token| !answer.contains(token.as_str()))
        .collect()
}

/// Tokens in `text` that are code rather than words.
///
/// A model translating a Korean sentence full of English identifiers turned
/// `LedgerDataSheetService` into 레저데이터시트서비스 on a reader's card. The
/// system prompt already says identifiers stay verbatim, and a 7B model
/// applying that to a bare CamelCase word in the middle of a sentence is
/// evidently not something a sentence of instruction achieves.
///
/// So they are named. Listing the actual tokens turns a rule the model has to
/// apply into a list it has to copy, which is a much easier thing to ask.
///
/// # What counts, and why the shapes are narrow
///
/// Only tokens no ordinary word could be: an underscore inside, a lower-then-
/// upper camel hump, a slash with something either side, or a scheme. A rule
/// that also caught capitalised words would list every proper noun in the
/// sentence and teach the model to leave names untranslated, which is the
/// opposite of what a reader wants.
///
/// # These came off a screen
///
/// So they are filtered the way [`crate::translate`]'s focused word is: no
/// quotes, no newlines, bounded length, bounded count. A token that survives
/// that cannot close the string it sits in or carry a sentence of instruction,
/// which is the property the fence exists to protect
/// (`docs/privacy-model.md`).
pub fn identifiers_in(text: &str) -> Vec<String> {
    /// Everything a token may contain besides letters and digits.
    ///
    /// A **closed** list. The first version removed quotes, newlines and
    /// backslashes and kept the rest, which is a list somebody has to keep
    /// complete forever against strings arriving from a stranger's screen.
    /// This one cannot be incomplete: a character not written here does not
    /// survive.
    const IDENTIFIER_PUNCTUATION: [char; 8] = ['_', '-', '/', '.', ':', '{', '}', '#'];

    /// Longest token worth naming. Past this it is not an identifier, it is a
    /// paragraph with no spaces in it.
    const LONGEST: usize = 64;
    /// Most tokens to name. A line of pure code would otherwise put a hundred
    /// of them in front of a model that has a sentence to translate.
    const MOST: usize = 8;

    let mut found: Vec<String> = Vec::new();
    for raw in text.split_whitespace() {
        // Kept characters rather than removed ones. A list of what may not
        // appear is a list somebody has to keep complete; a list of what may is
        // closed by construction, and these strings came off a screen.
        let token: String = raw
            .trim_matches(|c: char| !c.is_alphanumeric() && c != '/' && c != '_')
            .chars()
            .filter(|c| c.is_alphanumeric() || IDENTIFIER_PUNCTUATION.contains(c))
            .take(LONGEST)
            .collect();
        if token.chars().count() < 3 || token.chars().count() >= LONGEST {
            continue;
        }
        if !looks_like_code(&token) {
            continue;
        }
        if !found.iter().any(|kept| kept == &token) {
            found.push(token);
        }
        if found.len() >= MOST {
            break;
        }
    }
    found
}

/// Whether a token is code rather than a word.
fn looks_like_code(token: &str) -> bool {
    let has_letter = token.chars().any(char::is_alphabetic);
    if !has_letter {
        return false;
    }
    if token.contains("://") {
        return true;
    }
    // A path rather than a sentence that happens to contain a slash. Both
    // sides have to be there, **or** it begins with one, which is what an
    // absolute route looks like and what the first version of this missed:
    // `/projects/{id}/orbbase/tables/{table}/export` has nothing before its
    // first slash, so a rule about both sides rejected the longest and most
    // fragile identifier on the card.
    if let Some(rest) = token.strip_prefix('/') {
        if !rest.is_empty() {
            return true;
        }
    }
    if let Some((before, after)) = token.split_once('/') {
        if !before.is_empty() && !after.is_empty() {
            return true;
        }
    }
    if token.contains('_') {
        return true;
    }
    // A camel hump. `LedgerDataSheetService` has one and `English` does not,
    // which is the whole distinction: an initial capital is a sentence
    // starting, and a capital after a lower-case letter is not.
    token
        .chars()
        .zip(token.chars().skip(1))
        .any(|(first, second)| first.is_lowercase() && second.is_uppercase())
}

/// Whether a text is mostly code tokens rather than prose.
///
/// A source that is nothing but identifiers has nothing to translate:
/// a model will transliterate them into the target script, producing a
/// card that looks like a translation but matches nothing.
/// `LedgerDataSheetService` alone became 레저데이터시트서비스; `windows-rs
/// 0.62` became windows-r스 0.62. Both are worse than no card.
///
/// The check is broader than [`identifiers_in`] on purpose: it also
/// counts version numbers (`0.62`) and hyphenated tokens (`windows-rs`)
/// that `looks_like_code` cannot distinguish from ordinary words like
/// `well-known`. A sentence containing one identifier among prose passes;
/// a token list that is eighty percent code does not.
fn mostly_identifiers(text: &str) -> bool {
    let non_ws: usize = text.chars().filter(|c| !c.is_whitespace()).count();
    if non_ws == 0 {
        return false;
    }

    let code_chars: usize = text
        .split_whitespace()
        .filter(|token| looks_like_identifier_or_version(token))
        .map(|token| token.chars().filter(|c| !c.is_whitespace()).count())
        .sum();

    code_chars * 100 >= non_ws * 80
}

/// Whether a whitespace-delimited token is an identifier or version string.
///
/// Narrower than `identifiers_in` (which filters out `windows-rs` because
/// `looks_like_code` does not know hyphens) and broader at the same time
/// (it also catches version numbers like `0.62`). The two together cover
/// the shapes that reach this check: a lone CamelCase name, a hyphenated
/// crate name, a version, or any combination of those.
fn looks_like_identifier_or_version(token: &str) -> bool {
    if token.is_empty() {
        return false;
    }
    // A version number: starts with a digit, contains only digits and dots.
    if token.chars().next().is_some_and(|c| c.is_ascii_digit())
        && token.chars().all(|c| c.is_ascii_digit() || c == '.')
    {
        return true;
    }
    // Anything `identifiers_in` would name.
    if looks_like_code(token) {
        return true;
    }
    // A hyphenated name the camel/underscore/slash checks miss: at least
    // one letter, at least one hyphen with letters on both sides, and no
    // spaces. This catches `windows-rs` without catching `well-known`,
    // because `well-known` is all lower-case and has no other code signal;
    // but in the `mostly_identifiers` context it only matters when the
    // rest of the source is also code-like, which is the outer check.
    let has_letter = token.chars().any(char::is_alphabetic);
    let has_hyphen = token.contains('-');
    let all_code_chars = token
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '.' || c == '_');
    has_letter && has_hyphen && all_code_chars
}

#[cfg(test)]
mod identifier_tests {

    /// The token the prompt named and the model rewrote anyway.
    #[test]
    fn a_transliterated_identifier_is_reported_lost() {
        let source = "The LedgerDataSheetService returns one row per sheet.";
        let answer = "레저데이터시트서비스는 한 장당 한 행씩 반환합니다.";
        assert_eq!(
            identifiers_lost(source, answer),
            vec!["LedgerDataSheetService".to_owned()],
        );
    }

    /// A particle attached to a surviving identifier is not a loss.
    ///
    /// Korean writes `export_csv를`, and a rule about whole words would call
    /// the answer broken every time it was right.
    #[test]
    fn an_identifier_wearing_a_particle_survived() {
        let source = "Call export_csv once the rows are ready.";
        let answer = "행이 준비되면 한번만 export_csv를 호출하세요.";
        assert!(identifiers_lost(source, answer).is_empty());
    }

    /// Nothing named, nothing to lose.
    ///
    /// `windows-rs` is not an identifier by `identifiers_in`, so it is not
    /// checked here either. Teaching that rule about the hyphen would also
    /// teach it that `well-known` must be kept verbatim, which is the same
    /// shape and an ordinary word, so the two cases cannot be separated by
    /// punctuation and this does not try.
    #[test]
    fn a_hyphenated_name_is_not_watched() {
        let source = "The adapter is built on windows-rs 0.62.";
        let answer = "어댑터는 windows-r스 0.62에 기반합니다.";
        assert!(identifiers_lost(source, answer).is_empty());
    }

    #[test]
    fn an_ordinary_sentence_loses_nothing() {
        let source = "The output files came back empty.";
        let answer = "출력 파일은 비어있었습니다.";
        assert!(identifiers_lost(source, answer).is_empty());
    }

    /// Which of these does the prompt actually get told to keep.
    ///
    /// A lead from #207: `windows-rs 0.62` came back as `windows-r스 0.62`, one
    /// character inside the name transliterated. `looks_like_code` recognises
    /// an underscore, a slash, a scheme and a camel hump, and a hyphen is on
    /// none of those lists, so the token may never have been named at all.
    #[test]
    fn what_the_prompt_is_told_to_keep() {
        for (text, expected) in [
            ("LedgerDataSheetService", true),
            ("export_csv", true),
            ("/projects/{id}/export", true),
            ("https://example.com/a", true),
            ("windows-rs 0.62", false),
            ("qwen2.5:7b", false),
            ("Mori Lens", false),
            ("stale", false),
        ] {
            let found = identifiers_in(text);
            assert_eq!(!found.is_empty(), expected, "{text:?} produced {found:?}",);
        }
    }
    use super::{identifiers_in, identifiers_lost};

    /// The line from the card, and the token it damaged.
    #[test]
    fn the_identifiers_a_model_transliterated_are_found() {
        let line = "orbita의 LedgerDataSheetService 에 export_csv 를 넣고, POST /projects/{id}/orbbase/tables/{table}/export 라우트를 붙였습니다.";
        let found = identifiers_in(line);
        assert!(
            found.iter().any(|token| token == "LedgerDataSheetService"),
            "{found:?}"
        );
        assert!(found.iter().any(|token| token == "export_csv"), "{found:?}");
        assert!(
            found.iter().any(|token| token.starts_with("/projects/")),
            "{found:?}",
        );
    }

    /// And ordinary words are not, however capitalised.
    #[test]
    fn words_are_not_identifiers() {
        let line = "The Korean reader hovered a sentence in Chrome and Word on Tuesday.";
        assert!(
            identifiers_in(line).is_empty(),
            "{:?}",
            identifiers_in(line)
        );
    }

    /// A capital that starts a sentence is not a camel hump.
    #[test]
    fn a_leading_capital_is_not_a_hump() {
        assert!(identifiers_in("English German Korean").is_empty());
        assert_eq!(identifiers_in("camelCase"), vec!["camelCase".to_owned()]);
    }

    #[test]
    fn a_url_counts_and_a_sentence_with_a_slash_does_not() {
        assert_eq!(
            identifiers_in("see https://example.com/docs for more"),
            vec!["https://example.com/docs".to_owned()],
        );
        assert!(identifiers_in("the input / output boundary").is_empty());
    }

    /// Bounded in both directions, because these came off a screen.
    #[test]
    fn the_list_is_bounded_in_count_and_in_length() {
        let many: String = (0..40).map(|n| format!("token_{n} ")).collect();
        assert_eq!(identifiers_in(&many).len(), 8);

        let enormous = format!("prefix_{}", "x".repeat(500));
        assert!(
            identifiers_in(&enormous).is_empty(),
            "a paragraph is not a token"
        );
    }

    /// Nothing that could close the string it will sit in.
    #[test]
    fn a_token_carrying_a_quote_or_a_newline_is_stripped_of_it() {
        let hostile = "field_\"one\" second_\ntwo";
        for token in identifiers_in(hostile) {
            assert!(!token.contains('"'), "{token:?}");
            assert!(!token.contains('\n'), "{token:?}");
        }
    }

    use super::{mostly_identifiers, QuickRequest};

    /// The two inputs from #207 that were transliterated. Both must be
    /// refused by `is_worth_translating`, which is what the translation
    /// path checks before calling the model.
    #[test]
    fn a_source_that_is_entirely_an_identifier_is_not_worth_translating() {
        let request = QuickRequest::new("LedgerDataSheetService", "Korean");
        assert!(
            !request.is_worth_translating(),
            "a bare identifier must not reach the model"
        );
    }

    #[test]
    fn a_source_that_is_a_hyphenated_name_and_version_is_not_worth_translating() {
        let request = QuickRequest::new("windows-rs 0.62", "Korean");
        assert!(
            !request.is_worth_translating(),
            "a crate name and version must not reach the model"
        );
    }

    /// A sentence containing one identifier among prose is still worth
    /// translating. The refusal must only fire when identifiers dominate.
    #[test]
    fn an_ordinary_sentence_with_one_identifier_is_still_worth_translating() {
        let request = QuickRequest::new("The adapter is built on windows-rs 0.62.", "Korean");
        assert!(
            request.is_worth_translating(),
            "a sentence containing an identifier is still prose"
        );
    }

    #[test]
    fn a_sentence_with_a_camel_identifier_is_still_worth_translating() {
        let request = QuickRequest::new(
            "The LedgerDataSheetService returns one row per sheet.",
            "Korean",
        );
        assert!(request.is_worth_translating());
    }

    /// The `mostly_identifiers` function itself, tested at the boundary.
    #[test]
    fn mostly_identifiers_threshold() {
        assert!(mostly_identifiers("LedgerDataSheetService"));
        assert!(mostly_identifiers("windows-rs 0.62"));
        assert!(mostly_identifiers("export_csv"));
        assert!(!mostly_identifiers(
            "Call export_csv once the rows are ready."
        ));
        assert!(!mostly_identifiers("The output files came back empty."));
        assert!(!mostly_identifiers(""));
        assert!(!mostly_identifiers("English"));
    }
}

#[cfg(test)]
mod focus_tests {
    use super::*;

    /// A word is a word. The whole sentence is not.
    ///
    /// From a reader's log on 2026-08-07: `ocr 94 chars on the line, focus 94
    /// chars`. The recogniser had merged a line into one "word", so the prompt
    /// quoted the sentence back at the model as a hint about itself.
    #[test]
    fn a_focus_that_is_the_whole_sentence_is_not_a_focus() {
        let sentence = "The pointer must rest before anything is read.";
        let request = QuickRequest::new(sentence, "Korean").with_focus(sentence);
        assert_eq!(request.focused_word, None);
    }

    /// Line-wrapping artefacts must not make it look like a different string.
    ///
    /// Accessibility text arrives with them and a recognised word does not, so
    /// the comparison normalises whitespace exactly as the cache key does.
    #[test]
    fn whitespace_does_not_make_the_sentence_a_different_string() {
        let request = QuickRequest::new("The pointer   must\nrest.", "Korean")
            .with_focus("The pointer must rest.");
        assert_eq!(request.focused_word, None);
    }

    /// A real word still reaches the prompt, which is the half a strict rule
    /// could break.
    #[test]
    fn a_real_word_is_kept() {
        let request = QuickRequest::new("The pointer must rest before anything is read.", "Korean")
            .with_focus("rest");
        assert_eq!(request.focused_word.as_deref(), Some("rest"));
    }

    /// A one-word sentence has no useful focus either, and dropping it costs
    /// nothing: the model already has the word as the whole question.
    #[test]
    fn a_one_word_sentence_needs_no_hint_about_itself() {
        let request = QuickRequest::new("vacuously", "Korean").with_focus("vacuously");
        assert_eq!(request.focused_word, None);
    }

    #[test]
    fn an_empty_focus_is_no_focus() {
        let request = QuickRequest::new("a sentence here", "Korean").with_focus("   ");
        assert_eq!(request.focused_word, None);
    }
}

#[cfg(test)]
mod fence_tests {
    use super::{without_the_fence, FENCE_OPENING};

    const FENCE: &str = "#LENS-5a23bfc686af5a74#";

    /// The answer a reader actually got, on 2026-08-18.
    ///
    /// The closing marker is cut short because the token budget ran out inside
    /// it, which is why a rule about whole markers is not enough.
    #[test]
    fn the_frame_comes_off_and_the_answer_stays() {
        let answered = "#LENS-5a23bfc686af5a74#
공백
#LENS-5a23bfc686af5a7";
        assert_eq!(without_the_fence(answered, FENCE), "공백");
    }

    #[test]
    fn an_answer_with_no_marker_is_untouched() {
        for text in ["공백", "출력 파일은 비어있었습니다.", "windows-rs 0.62"] {
            assert_eq!(without_the_fence(text, FENCE), text);
        }
    }

    /// Somebody hovering this repository's own source keeps their text.
    ///
    /// `#LENS-` is a string that exists in this tree, so a rule about the
    /// opening alone would cut a reader's own screen out of their card. Only
    /// this request's fence is cut.
    #[test]
    fn another_fence_is_somebody_elses_text() {
        let text = "the marker is #LENS-0000000000000000# in that run";
        assert_eq!(without_the_fence(text, FENCE), text);
        let bare = format!("a constant called {FENCE_OPENING} lives here");
        assert_eq!(without_the_fence(&bare, FENCE), bare);
    }

    /// Nothing but the frame is nothing, and the caller turns that into an
    /// error rather than a card with an empty body.
    #[test]
    fn a_reply_that_is_only_the_frame_is_empty() {
        let only = format!(
            "{FENCE}
{FENCE}"
        );
        assert_eq!(without_the_fence(&only, FENCE), "");
    }

    /// A fence too short to be one is not used as a needle.
    ///
    /// Defensive rather than reachable: `fence_for` always produces sixteen
    /// hexadecimal digits. It is here because the alternative is a function
    /// that would cut on a two-character needle if one were ever passed.
    #[test]
    fn a_short_fence_cuts_nothing() {
        assert_eq!(
            without_the_fence("#LENS-1# 공백", "#LENS-1#"),
            "#LENS-1# 공백"
        );
    }
}
