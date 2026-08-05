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

use crate::translate::{QuickRequest, QuickTranslation};

/// What makes two requests the same question.
///
/// Every part is load-bearing. Drop the model and a cache filled by a 3B model
/// answers for a 27B one. Drop the prompt version and a change to the
/// instructions silently keeps serving answers shaped by the old ones. Drop the
/// focused word and pointing at a different word in the same sentence returns
/// the previous word's sense.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CacheKey {
    source: String,
    focused_word: Option<String>,
    target_language: String,
    model: String,
    prompt_version: u32,
}

impl CacheKey {
    pub fn of(request: &QuickRequest, model: &str, prompt_version: u32) -> Self {
        Self {
            // Normalised, so the same sentence with different surrounding
            // whitespace is one entry. Nothing else is folded: case and
            // punctuation change meaning, and a cache that ignores them
            // answers a question nobody asked.
            source: normalise(&request.source_text),
            focused_word: request.focused_word.as_deref().map(normalise),
            target_language: request.target_language.clone(),
            model: model.to_owned(),
            prompt_version,
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
pub struct TranslationCache {
    entries: HashMap<CacheKey, Entry>,
    capacity: usize,
    /// Monotonic, so the least recently used entry is the one with the lowest
    /// stamp. A clock is not used: it would make the cache untestable without
    /// sleeping, and it buys nothing here.
    tick: u64,
    hits: u64,
    misses: u64,
}

#[derive(Debug)]
struct Entry {
    translation: QuickTranslation,
    used_at: u64,
}

impl TranslationCache {
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
    pub fn get(&mut self, key: &CacheKey) -> Option<QuickTranslation> {
        self.tick += 1;
        let tick = self.tick;
        match self.entries.get_mut(key) {
            Some(entry) => {
                entry.used_at = tick;
                self.hits += 1;
                let mut answer = entry.translation.clone();
                // The measured latency is kept. What changes is that the card
                // now knows not to present it as this hover's cost.
                answer.cached = true;
                Some(answer)
            }
            None => {
                self.misses += 1;
                None
            }
        }
    }

    pub fn insert(&mut self, key: CacheKey, translation: QuickTranslation) {
        self.tick += 1;
        if self.entries.len() >= self.capacity && !self.entries.contains_key(&key) {
            self.evict_oldest();
        }
        self.entries.insert(
            key,
            Entry {
                translation,
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
            "the measured latency was overwritten, which is how 0ms reached              the screen",
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

    #[test]
    fn pointing_at_a_different_word_is_a_different_question() {
        // The focused word changes which sense the model picks, so it must
        // change the key. Otherwise hovering `produce` returns the answer
        // shaped for `suite`.
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
        assert!(cache.get(&on_suite).is_none());
        assert!(cache.get(&on_produce).is_some());
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
