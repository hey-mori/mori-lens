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
use crate::translate::{Provider, QuickRequest, QuickTranslation};

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
    provider: Provider,
    model: String,
    prompt_version: u32,
    /// The word the reader pointed at. Always `None` on the quick path; see
    /// the note above.
    focus: Option<String>,
    /// Who was reading. Always `None` on the quick path, which has no profile.
    ///
    /// Here because a profile changes the prompt, so an explanation made for a
    /// developer is not the answer an examination reader asked for. Without
    /// this, changing the setting and hovering the same sentence would return
    /// the old card in seven microseconds and look like it had worked.
    profile: Option<&'static str>,
}

impl CacheKey {
    pub fn of(
        request: &QuickRequest,
        provider: Provider,
        model: &str,
        prompt_version: u32,
    ) -> Self {
        Self {
            asked: Asked::ToTranslate,
            focus: None,
            profile: None,
            // Normalised, so the same sentence with different surrounding
            // whitespace is one entry. Nothing else is folded: case and
            // punctuation change meaning, and a cache that ignores them
            // answers a question nobody asked.
            source: normalise(&request.source_text),
            target_language: request.target_language.clone(),
            provider,
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
    pub fn for_analysis(
        request: &DeepRequest,
        provider: Provider,
        model: &str,
        prompt_version: u32,
    ) -> Self {
        Self {
            asked: Asked::ToExplain,
            source: normalise(&request.sentence),
            target_language: request.target_language.clone(),
            provider,
            model: model.to_owned(),
            prompt_version,
            focus: request.word.as_deref().map(normalise),
            profile: Some(request.profile.as_str()),
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
            engine: EngineId::Local,
            model: "qwen2.5:3b".to_owned(),
            latency_ms,
            cached: false,
            part_was_removed: false,
            name_changed: false,
        }
    }

    fn key(source: &str) -> CacheKey {
        CacheKey::of(
            &QuickRequest::new(source, "Korean"),
            Provider::Ollama,
            "qwen2.5:3b",
            1,
        )
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
            Provider::Ollama,
            "qwen2.5:3b",
            1,
        );
        let on_suite = CacheKey::of(
            &QuickRequest::new(sentence, "Korean").with_focus("suite"),
            Provider::Ollama,
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
            Provider::Ollama,
            "qwen2.5:3b",
            1,
        );
        let second = CacheKey::of(
            &QuickRequest::new("The suite produced nothing at all.", "Korean"),
            Provider::Ollama,
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
        cache.insert(
            CacheKey::of(&request, Provider::Ollama, "qwen2.5:3b", 1),
            answer("가", 1),
        );

        assert!(cache
            .get(&CacheKey::of(&request, Provider::Ollama, "gemma4:e4b", 1))
            .is_none());
        assert!(cache
            .get(&CacheKey::of(&request, Provider::Ollama, "qwen2.5:3b", 2))
            .is_none());
    }

    /// The model-switch isolation guarantee.
    ///
    /// A reader changes the Quick model in settings. The same sentence is
    /// hovered again. The cache must NOT serve the old model's answer: the
    /// new model may produce different wording, different emphasis, or a
    /// different language register. Serving a stale answer would look like
    /// it worked (the card appears instantly) but would be the wrong model's
    /// output. This test proves the cache miss forces a fresh model call.
    #[test]
    fn switching_the_quick_model_invalidates_the_cache_for_the_same_sentence() {
        let mut cache = TranslationCache::new(8);
        let sentence = "The pointer must rest before anything is read.";

        // Model A translates the sentence.
        let model_a_key = CacheKey::of(
            &QuickRequest::new(sentence, "Korean"),
            Provider::Ollama,
            "qwen2.5:3b",
            1,
        );
        cache.insert(model_a_key, answer("포인터가 멈춰야 합니다.", 2400));

        // The reader switches to model B. Same sentence, same prompt version.
        let model_b_key = CacheKey::of(
            &QuickRequest::new(sentence, "Korean"),
            Provider::Ollama,
            "gemma4:e4b",
            1,
        );

        // Cache miss: model B's answer must come from a fresh model call.
        assert!(
            cache.get(&model_b_key).is_none(),
            "switching models must not serve the previous model's cached answer",
        );

        // Model B produces its own answer, which is now cached.
        cache.insert(model_b_key.clone(), answer("포인터가 쉬어야 합니다.", 1800));
        let hit = cache.get(&model_b_key).expect("model B answer is cached");
        assert_eq!(hit.text, "포인터가 쉬어야 합니다.");

        // Model A's answer is still in the cache (not overwritten).
        let model_a_again = CacheKey::of(
            &QuickRequest::new(sentence, "Korean"),
            Provider::Ollama,
            "qwen2.5:3b",
            1,
        );
        let hit_a = cache
            .get(&model_a_again)
            .expect("model A answer still cached");
        assert_eq!(hit_a.text, "포인터가 멈춰야 합니다.");
    }

    #[test]
    fn a_different_target_language_is_a_different_question() {
        let mut cache = TranslationCache::new(8);
        cache.insert(
            CacheKey::of(
                &QuickRequest::new("hello", "Korean"),
                Provider::Ollama,
                "m",
                1,
            ),
            answer("안녕", 1),
        );
        assert!(cache
            .get(&CacheKey::of(
                &QuickRequest::new("hello", "Japanese"),
                Provider::Ollama,
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
        let rest = CacheKey::for_analysis(&a_deep_request(Some("rest")), Provider::Ollama, "m", 1);
        let read = CacheKey::for_analysis(&a_deep_request(Some("read")), Provider::Ollama, "m", 1);
        assert_ne!(rest, read);
    }

    /// And the quick path's rule still holds, which is what makes the pair a
    /// deliberate asymmetry rather than an oversight.
    #[test]
    fn a_different_word_in_one_sentence_is_the_same_translation() {
        let sentence = "The pointer must rest before anything is read.";
        let rest = CacheKey::of(
            &QuickRequest::new(sentence, "Korean").with_focus("rest"),
            Provider::Ollama,
            "m",
            1,
        );
        let read = CacheKey::of(
            &QuickRequest::new(sentence, "Korean").with_focus("read"),
            Provider::Ollama,
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
        let quick = CacheKey::of(
            &QuickRequest::new(sentence, "Korean"),
            Provider::Ollama,
            "m",
            1,
        );
        let deep = CacheKey::for_analysis(&a_deep_request(None), Provider::Ollama, "m", 1);
        assert_ne!(quick, deep);
    }

    /// A remembered explanation says it was remembered.
    #[test]
    fn an_explanation_from_memory_is_marked_as_such() {
        let mut cache = AnalysisCache::new(4);
        let key = CacheKey::for_analysis(&a_deep_request(Some("rest")), Provider::Ollama, "m", 1);
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
            CacheKey::for_analysis(&request, Provider::Ollama, "m", 1),
            CacheKey::for_analysis(&request, Provider::Ollama, "m", 2),
        );
    }

    /// Switching the Deep model invalidates the cache for the same sentence
    /// and focus word. Same guarantee as the quick path, different key
    /// construction (the focus word IS part of the deep key).
    #[test]
    fn switching_the_deep_model_invalidates_the_cache_for_the_same_word() {
        let mut cache = AnalysisCache::new(8);
        let request = a_deep_request(Some("rest"));

        let key_a = CacheKey::for_analysis(&request, Provider::Ollama, "qwen2.5:7b", 1);
        cache.insert(
            key_a,
            DeepAnalysis {
                lemma: "rest".to_owned(),
                latency_ms: 4400,
                ..DeepAnalysis::default()
            },
        );

        let key_b = CacheKey::for_analysis(&request, Provider::Ollama, "gemma3:12b", 1);
        assert!(
            cache.get(&key_b).is_none(),
            "switching deep models must not serve the previous model's cached analysis",
        );
    }
}

#[cfg(test)]
mod profile_key_tests {
    use super::*;
    use crate::analyse::Profile;

    /// Two profiles are two questions, so they are two entries.
    ///
    /// Without this the deep cache would answer a developer's hover with the
    /// examination reader's card, in seven microseconds, indistinguishably from
    /// having worked.
    #[test]
    fn a_profile_change_is_a_different_key() {
        let of = |profile: Profile| {
            CacheKey::for_analysis(
                &DeepRequest::new("The lease is renewed.", "임대는 갱신됩니다.", "Korean")
                    .for_reader(profile),
                Provider::Ollama,
                "qwen2.5:7b",
                1,
            )
        };
        assert_ne!(of(Profile::Reader), of(Profile::Developer));
        assert_ne!(of(Profile::Developer), of(Profile::Exam));
        assert_eq!(of(Profile::Exam), of(Profile::Exam));
    }

    /// And the quick path is untouched, because it has no profile to carry.
    #[test]
    fn a_translation_key_carries_no_profile() {
        let key = CacheKey::of(
            &crate::translate::QuickRequest::new("The lease is renewed.", "Korean"),
            Provider::Ollama,
            "qwen2.5:7b",
            1,
        );
        assert_eq!(key.profile, None);
    }
}

#[cfg(test)]
mod provider_identity_tests {
    use super::*;
    use crate::translate::QuickRequest;

    /// P6: Provider identity is part of cache key.
    ///
    /// Same model ID from different providers must not share cache entries.
    /// A user running Ollama and LM Studio with the same model name (e.g.,
    /// "qwen2.5:7b") would see LM Studio's answer served for Ollama hovers
    /// if provider were not in the key. The two providers have different
    /// endpoints, different inference characteristics, and potentially
    /// different model weights even when the name matches.
    #[test]
    fn different_producers_with_same_model_are_different_keys() {
        let request = QuickRequest::new("The suite produces nothing.", "Korean");
        let ollama_key = CacheKey::of(&request, Provider::Ollama, "qwen2.5:7b", 1);
        let lmstudio_key = CacheKey::of(&request, Provider::LmStudio, "qwen2.5:7b", 1);
        assert_ne!(
            ollama_key, lmstudio_key,
            "same model ID from different providers must not share cache entries",
        );
    }

    /// The deep path carries provider identity too.
    #[test]
    fn deep_keys_differ_by_provider() {
        let request = DeepRequest::new("The lease is renewed.", "임대는 갱신됩니다.", "Korean");
        let ollama_key = CacheKey::for_analysis(&request, Provider::Ollama, "qwen2.5:7b", 1);
        let lmstudio_key = CacheKey::for_analysis(&request, Provider::LmStudio, "qwen2.5:7b", 1);
        assert_ne!(
            ollama_key, lmstudio_key,
            "deep analysis keys must differ by provider",
        );
    }

    /// A cache filled by one provider does not serve the other.
    #[test]
    fn cache_filled_by_one_provider_does_not_serve_the_other() {
        let mut cache = TranslationCache::new(8);
        let request = QuickRequest::new("The suite produces nothing.", "Korean");

        let ollama_key = CacheKey::of(&request, Provider::Ollama, "qwen2.5:7b", 1);
        cache.insert(
            ollama_key,
            QuickTranslation {
                text: "아무것도 생성하지 않습니다.".to_owned(),
                engine: crate::translate::EngineId::Local,
                model: "qwen2.5:7b".to_owned(),
                latency_ms: 2400,
                cached: false,
                part_was_removed: false,
                name_changed: false,
            },
        );

        let lmstudio_key = CacheKey::of(&request, Provider::LmStudio, "qwen2.5:7b", 1);
        assert!(
            cache.get(&lmstudio_key).is_none(),
            "LM Studio hover must not receive Ollama's cached translation",
        );
    }
}
