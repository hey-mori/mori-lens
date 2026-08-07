//! Remembering what has already been translated.
//!
//! A reader moves along a line, back to a word, forward again. The same
//! sentence is hovered several times a minute, and every one of those costs a
//! second and a half of model time for an answer that has not changed.
//!
//! In memory only, and gone when the process exits. A cache that survives a
//! restart is a record of what the user read, which is exactly the file
//! `docs/privacy-model.md` promises not to keep without asking. A durable tier
//! is possible later and it is an opt-in with its own decision, not a
//! performance detail.

use std::collections::HashMap;

use crate::analyse::{DeepAnalysis, DeepRequest};
use crate::translate::{QuickRequest, QuickTranslation};

/// An answer a cache can hand back and mark as not having cost anything.
///
/// The trait exists so one map-plus-a-counter serves both paths of
/// [ADR-0005](../../../docs/decisions/ADR-0005-two-speed-ai.md). What differs
/// between them is the key and the value, and neither of those is the
/// eviction rule.
pub trait Remembered: Clone {
    /// Say that this came back without a model call.
    ///
    /// The card footer reports what just happened, and a remembered answer's
    /// original latency is a number about a press the reader has forgotten.
    fn mark_cached(&mut self);
}

impl Remembered for QuickTranslation {
    fn mark_cached(&mut self) {
        self.cached = true;
    }
}

impl Remembered for DeepAnalysis {
    fn mark_cached(&mut self) {
        self.cached = true;
    }
}

/// Which path a key belongs to.
///
/// In the key rather than only in which cache holds it. The two are separate
/// maps today, and a single key type that could describe both without saying
/// which would be one refactor away from answering a translation with an
/// explanation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Asked {
    ToTranslate,
    ToExplain,
}

/// What makes two requests the same question.
///
/// Every part is load-bearing. Drop the model and a cache filled by a 3B model
/// answers for a 27B one. Drop the prompt version and a change to the
/// instructions silently keeps serving answers shaped by the old ones.
///
/// # The focused word is not in it, and was
///
/// It was, on the reasoning that pointing at a different word in the same
/// sentence should not return the previous word's sense. That reasoning is
/// sound and the price turned out to be the product.
///
/// A reader scanning one sentence hovers word after word. Each word is a
/// different key, so each is a fresh two to three second translation of a
/// sentence already translated, and what they reported was that answers do not
/// arrive and that the same sentence sometimes comes back instantly and
/// sometimes does not. A run of it is in the log they sent: the same 194
/// characters answered twice, 2.8s and then 2.7s, with no hit between them.
///
/// The focused word is still **sent**, so the model still gets the hint. It is
/// no longer part of the question the cache thinks it is answering, which
/// means the first hover into a sentence decides its wording and every hover
/// after that is free.
///
/// What that costs: a second word in the same sentence gets a translation
/// nudged towards the first word's sense. The card shows a whole sentence
/// either way, so the difference is a shade of wording against a wait long
/// enough that a reader stops trusting the product.
/// # The focused word **is** in a deep key, and that is not a contradiction
///
/// The two paths answer different questions about the same sentence, so the
/// same field is load-bearing in one and harmful in the other.
///
/// A quick answer is a whole sentence. Point at another word in it and what
/// changes is a shade of wording, against a two to three second wait, which is
/// the trade above.
///
/// A deep answer is **about the word**: its dictionary form, its part of
/// speech, what it means here. Leaving the focus out would serve the previous
/// word's lemma for the next word. That is not a shade of wording, it is the
/// wrong answer, in the confident voice this whole path carries a label to
/// warn about.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CacheKey {
    asked: Asked,
    source: String,
    target_language: String,
    model: String,
    prompt_version: u32,
    /// The word the reader pointed at. Always `None` on the quick path; see
    /// the note above.
    focus: Option<String>,
}

impl CacheKey {
    pub fn of(request: &QuickRequest, model: &str, prompt_version: u32) -> Self {
        Self {
            asked: Asked::ToTranslate,
            focus: None,
            // Normalised, so the same sentence with different surrounding
            // whitespace is one entry. Nothing else is folded: case and
            // punctuation change meaning, and a cache that ignores them
            // answers a question nobody asked.
            source: normalise(&request.source_text),
            target_language: request.target_language.clone(),
            model: model.to_owned(),
            prompt_version,
        }
    }

    /// The key for an explanation.
    ///
    /// Its own prompt version, because the two prompts change independently
    /// and a version shared between them would throw away every translation
    /// whenever a heading was reworded.
    ///
    /// The translation this was grounded on is deliberately **not** in the
    /// key. It is derived from the source, the target and the model, all three
    /// of which are already here, so including it would add nothing and would
    /// let a streamed answer's last-token wobble evict an explanation.
    pub fn for_analysis(request: &DeepRequest, model: &str, prompt_version: u32) -> Self {
        Self {
            asked: Asked::ToExplain,
            source: normalise(&request.sentence),
            target_language: request.target_language.clone(),
            model: model.to_owned(),
            prompt_version,
            focus: request.word.as_deref().map(normalise),
        }
    }
}

/// Collapse runs of whitespace and trim. Accessibility text arrives with
/// line-wrapping artefacts that are not part of the sentence.
fn normalise(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A bounded, in-memory translation cache with least-recently-used eviction.
///
/// Hand-written rather than a crate: the whole thing is a map plus a counter,
/// and it keeps `lens-core` dependency-free, which is what lets the macOS
/// adapter be a new file rather than a new dependency graph.
#[derive(Debug)]
pub struct Cache<V: Remembered> {
    entries: HashMap<CacheKey, Entry<V>>,
    capacity: usize,
    /// Monotonic, so the least recently used entry is the one with the lowest
    /// stamp. A clock is not used: it would make the cache untestable without
    /// sleeping, and it buys nothing here.
    tick: u64,
    hits: u64,
    misses: u64,
}

/// The quick path's cache. The name every caller already uses.
pub type TranslationCache = Cache<QuickTranslation>;

/// The deep path's cache. A separate map with separate keys, per ADR-0005.
pub type AnalysisCache = Cache<DeepAnalysis>;

#[derive(Debug)]
struct Entry<V> {
    value: V,
    used_at: u64,
}

impl<V: Remembered> Cache<V> {
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: HashMap::new(),
            capacity: capacity.max(1),
            tick: 0,
            hits: 0,
            misses: 0,
        }
    }

    /// Look up a translation, marking it as just used.
    ///
    /// The returned copy has `latency_ms` set to zero and is otherwise the
    /// original answer. Reporting the original latency would put a made-up
    /// number on the card: the user waited nothing this time, and the card
    /// footer is supposed to say what just happened.
    pub fn get(&mut self, key: &CacheKey) -> Option<V> {
        self.tick += 1;
        let tick = self.tick;
        match self.entries.get_mut(key) {
            Some(entry) => {
                entry.used_at = tick;
                self.hits += 1;
                let mut answer = entry.value.clone();
                // The measured latency is kept. What changes is that the card
                // now knows not to present it as this hover's cost.
                answer.mark_cached();
                Some(answer)
            }
            None => {
                self.misses += 1;
                None
            }
        }
    }

    pub fn insert(&mut self, key: CacheKey, value: V) {
        self.tick += 1;
        if self.entries.len() >= self.capacity && !self.entries.contains_key(&key) {
            self.evict_oldest();
        }
        self.entries.insert(
            key,
            Entry {
                value,
                used_at: self.tick,
            },
        );
    }

    fn evict_oldest(&mut self) {
        let oldest = self
            .entries
            .iter()
            .min_by_key(|(_, entry)| entry.used_at)
            .map(|(key, _)| key.clone());
        if let Some(key) = oldest {
            self.entries.remove(&key);
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Hits over lookups, as a percentage. One of the numbers
    /// `mori-lens-project-goal.md` §6 says success is measured by.
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 * 100.0 / total as f64
        }
    }

    pub fn hits(&self) -> u64 {
        self.hits
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::translate::EngineId;

    fn answer(text: &str, latency_ms: u128) -> QuickTranslation {
        QuickTranslation {
            text: text.to_owned(),
            engine: EngineId::LocalOllama,
            model: "qwen2.5:3b".to_owned(),
            latency_ms,
            cached: false,
        }
    }

    fn key(source: &str) -> CacheKey {
        CacheKey::of(&QuickRequest::new(source, "Korean"), "qwen2.5:3b", 1)
    }

    #[test]
    fn the_same_sentence_hovered_twice_is_answered_from_memory() {
        let mut cache = TranslationCache::new(8);
        let sentence = "The suite did not produce anything.";
        assert!(cache.get(&key(sentence)).is_none());

        cache.insert(key(sentence), answer("아무것도 생성하지 않았습니다.", 2400));
        let hit = cache.get(&key(sentence)).expect("should hit");
        assert_eq!(hit.text, "아무것도 생성하지 않았습니다.");
    }

    /// The footer says what just happened, and a cache hit did not take
    /// 2400ms. That was right, and the way it said so was not.
    ///
    /// This used to overwrite the latency with `0`, and the card printed
    /// `0ms`. A user saw a broken clock rather than a fast answer, and said
    /// so. Zero is not a different thing to say, it is a wrong one.
    ///
    /// So the measurement is kept as measured and a flag carries the fact,
    /// which lets the card print `캐시` instead of a duration.
    #[test]
    fn a_cached_answer_is_marked_rather_than_given_a_made_up_latency() {
        let mut cache = TranslationCache::new(8);
        cache.insert(key("something"), answer("무엇인가", 2400));

        let hit = cache.get(&key("something")).expect("should hit");
        assert!(hit.cached, "a hit did not say it was one");
        assert_eq!(
            hit.latency_ms, 2400,
            "the measured latency was overwritten, which is how 0ms reached the screen",
        );
    }

    #[test]
    fn an_answer_straight_from_the_engine_is_not_marked_cached() {
        assert!(!answer("무엇인가", 2400).cached);
    }

    #[test]
    fn line_wrapping_whitespace_does_not_split_one_sentence_into_two_entries() {
        // Accessibility text arrives with the wrapping of the window it came
        // from, so the same sentence read at two window widths must be one
        // entry.
        let mut cache = TranslationCache::new(8);
        cache.insert(
            key("The suite did not\n   produce anything."),
            answer("x", 1),
        );
        assert!(cache
            .get(&key("The suite did not produce anything."))
            .is_some());
    }

    /// Pointing at another word in the same sentence is the same question.
    ///
    /// **This test asserted the opposite until 2026-08-06**, on the reasoning
    /// that the focused word changes which sense the model picks. It does, and
    /// keying on it made every word in a sentence a fresh two to three second
    /// translation of a sentence already translated. A reader reported that
    /// answers were not arriving; a log of theirs has the same 194 characters
    /// answered twice, 2.8s then 2.7s, with no hit between them.
    ///
    /// The word is still sent to the model. It is no longer part of the
    /// question the cache thinks it is answering.
    #[test]
    fn pointing_at_a_different_word_is_the_same_question() {
        let mut cache = TranslationCache::new(8);
        let sentence = "The suite did not produce anything.";
        let on_produce = CacheKey::of(
            &QuickRequest::new(sentence, "Korean").with_focus("produce"),
            "qwen2.5:3b",
            1,
        );
        let on_suite = CacheKey::of(
            &QuickRequest::new(sentence, "Korean").with_focus("suite"),
            "qwen2.5:3b",
            1,
        );
        cache.insert(on_produce.clone(), answer("생성하다", 1));
        assert!(
            cache.get(&on_suite).is_some(),
            "the second word in a sentence paid for a whole translation again",
        );
        assert!(cache.get(&on_produce).is_some());
    }

    /// And a different sentence is still a different question.
    #[test]
    fn a_different_sentence_is_a_different_question() {
        let mut cache = TranslationCache::new(8);
        let first = CacheKey::of(
            &QuickRequest::new("The suite did not produce anything.", "Korean"),
            "qwen2.5:3b",
            1,
        );
        let second = CacheKey::of(
            &QuickRequest::new("The suite produced nothing at all.", "Korean"),
            "qwen2.5:3b",
            1,
        );
        cache.insert(first, answer("생성하다", 1));
        assert!(cache.get(&second).is_none());
    }

    #[test]
    fn a_different_model_or_prompt_does_not_reuse_the_old_answer() {
        let mut cache = TranslationCache::new(8);
        let request = QuickRequest::new("something", "Korean");
        cache.insert(CacheKey::of(&request, "qwen2.5:3b", 1), answer("가", 1));

        assert!(cache
            .get(&CacheKey::of(&request, "gemma4:e4b", 1))
            .is_none());
        assert!(cache
            .get(&CacheKey::of(&request, "qwen2.5:3b", 2))
            .is_none());
    }

    #[test]
    fn a_different_target_language_is_a_different_question() {
        let mut cache = TranslationCache::new(8);
        cache.insert(
            CacheKey::of(&QuickRequest::new("hello", "Korean"), "m", 1),
            answer("안녕", 1),
        );
        assert!(cache
            .get(&CacheKey::of(
                &QuickRequest::new("hello", "Japanese"),
                "m",
                1
            ))
            .is_none());
    }

    #[test]
    fn the_least_recently_used_entry_is_the_one_evicted() {
        let mut cache = TranslationCache::new(2);
        cache.insert(key("one"), answer("하나", 1));
        cache.insert(key("two"), answer("둘", 1));

        // Touch "one" so "two" becomes the oldest.
        assert!(cache.get(&key("one")).is_some());
        cache.insert(key("three"), answer("셋", 1));

        assert_eq!(cache.len(), 2);
        assert!(cache.get(&key("one")).is_some(), "the used entry survived");
        assert!(cache.get(&key("two")).is_none(), "the oldest was evicted");
    }

    #[test]
    fn re_inserting_a_present_key_does_not_evict_anything() {
        let mut cache = TranslationCache::new(2);
        cache.insert(key("one"), answer("하나", 1));
        cache.insert(key("two"), answer("둘", 1));
        cache.insert(key("one"), answer("하나!", 1));

        assert_eq!(cache.len(), 2);
        assert!(cache.get(&key("two")).is_some());
    }

    #[test]
    fn the_hit_rate_counts_lookups_not_insertions() {
        let mut cache = TranslationCache::new(8);
        assert_eq!(cache.hit_rate(), 0.0);
        cache.get(&key("miss"));
        cache.insert(key("hit"), answer("x", 1));
        cache.get(&key("hit"));
        assert_eq!(cache.hit_rate(), 50.0);
    }
}

#[cfg(test)]
mod two_path_tests {
    use super::*;

    fn a_deep_request(word: Option<&str>) -> DeepRequest {
        let request = DeepRequest::new(
            "The pointer must rest before anything is read.",
            "포인터가 멈춘 뒤에야 무엇이든 읽습니다.",
            "Korean",
        );
        match word {
            Some(word) => request.about_the_word(word),
            None => request,
        }
    }

    /// Pointing at a different word is a different question of the deep path.
    ///
    /// The opposite of the quick path's rule, and the reason both are written
    /// down. A deep answer is **about the word**: its dictionary form, its
    /// part of speech, what it means here. Serving the previous word's lemma
    /// for the next word is not a shade of wording, it is wrong.
    #[test]
    fn a_different_word_in_one_sentence_is_a_different_explanation() {
        let rest = CacheKey::for_analysis(&a_deep_request(Some("rest")), "m", 1);
        let read = CacheKey::for_analysis(&a_deep_request(Some("read")), "m", 1);
        assert_ne!(rest, read);
    }

    /// And the quick path's rule still holds, which is what makes the pair a
    /// deliberate asymmetry rather than an oversight.
    #[test]
    fn a_different_word_in_one_sentence_is_the_same_translation() {
        let sentence = "The pointer must rest before anything is read.";
        let rest = CacheKey::of(
            &QuickRequest::new(sentence, "Korean").with_focus("rest"),
            "m",
            1,
        );
        let read = CacheKey::of(
            &QuickRequest::new(sentence, "Korean").with_focus("read"),
            "m",
            1,
        );
        assert_eq!(rest, read);
    }

    /// One sentence cannot be both questions.
    ///
    /// The two live in separate maps, so this can only bite after a refactor.
    /// It is asserted because the failure it prevents is a translation served
    /// as an explanation, which no type in either path would notice.
    #[test]
    fn a_translation_key_and_an_explanation_key_are_never_equal() {
        let sentence = "The pointer must rest before anything is read.";
        let quick = CacheKey::of(&QuickRequest::new(sentence, "Korean"), "m", 1);
        let deep = CacheKey::for_analysis(&a_deep_request(None), "m", 1);
        assert_ne!(quick, deep);
    }

    /// A remembered explanation says it was remembered.
    #[test]
    fn an_explanation_from_memory_is_marked_as_such() {
        let mut cache = AnalysisCache::new(4);
        let key = CacheKey::for_analysis(&a_deep_request(Some("rest")), "m", 1);
        cache.insert(
            key.clone(),
            DeepAnalysis {
                lemma: "rest".to_owned(),
                latency_ms: 4400,
                ..DeepAnalysis::default()
            },
        );

        let hit = cache.get(&key).expect("a hit");
        assert!(hit.cached);
        // The measured figure is kept; what changes is that the card knows
        // not to present it as this press's cost.
        assert_eq!(hit.latency_ms, 4400);
    }

    /// Each prompt version is its own, so rewording one does not empty the
    /// other.
    #[test]
    fn the_two_prompt_versions_move_independently() {
        let request = a_deep_request(Some("rest"));
        assert_ne!(
            CacheKey::for_analysis(&request, "m", 1),
            CacheKey::for_analysis(&request, "m", 2),
        );
    }
}
