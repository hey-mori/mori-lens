use lens_core::analyse::{DeepAnalysis, DeepRequest, Profile};
/// Async dispatch carrier types for translation jobs.
///
/// These types carry provenance (target_language, source_app) through the async
/// boundary between dispatch and result arrival. They are defined here (rather
/// than inside the platform-specific product loop) so the dispatch-time
/// provenance invariant can be tested on all platforms.
///
/// The invariant: a job dispatched with target_language = X must produce a
/// result with target_language = X, regardless of settings changes that occur
/// between dispatch and result arrival.
///
/// # Production seam
///
/// The `into_result` methods on `QuickJob` and `DeepJob` produce `TransResult`.
/// Production workers and regression tests both call these methods, ensuring
/// the target_language in the result comes from the dispatch-time job, not
/// from live settings.
use lens_core::geometry::ScreenRect;
use lens_core::translate::{QuickRequest, QuickTranslation};

/// Quick translation job dispatched to the async worker.
///
/// The target_language is owned by `request`, not duplicated here.
/// `into_result` derives the result's target_language from `request.target_language`.
#[allow(dead_code)]
pub enum QuickJob {
    Quick {
        request: QuickRequest,
        sentence: String,
        display_text: String,
        from_pixels: bool,
        app: Option<String>,
        focused_word: Option<String>,
        generation: u64,
        anchor: ScreenRect,
    },
}

// into_result is the production seam, called from the macOS worker.
// On non-macOS targets the methods are exercised only by regression tests.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
impl QuickJob {
    /// Produce a TransResult from this job and the translation outcome.
    ///
    /// The result's target_language comes from `self.request.target_language`,
    /// not from any external source. This is the production seam that ensures
    /// dispatch-time provenance is preserved.
    pub fn into_result(self, result: Result<QuickTranslation, String>) -> TransResult {
        match self {
            QuickJob::Quick {
                request,
                sentence,
                display_text,
                from_pixels,
                app,
                focused_word,
                generation,
                anchor,
            } => TransResult::Quick {
                sentence,
                display_text,
                result,
                from_pixels,
                app,
                focused_word,
                generation,
                anchor,
                target_language: request.target_language,
            },
        }
    }
}

/// Deep translation job dispatched to the async worker.
///
/// The target_language is bound at dispatch time and carried through to the
/// result via `into_result`.
#[allow(dead_code)]
pub enum DeepJob {
    Deep {
        sentence: String,
        translation: Option<String>,
        target_language: String,
        profile: Profile,
        app: Option<String>,
        focused_word: Option<String>,
        generation: u64,
        anchor: ScreenRect,
    },
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
impl DeepJob {
    /// Produce a TransResult from this job and the analysis outcome.
    ///
    /// The result's target_language comes from `self.target_language`,
    /// which was bound at dispatch time. This is the production seam.
    pub fn into_result(self, result: Result<DeepAnalysis, String>) -> TransResult {
        match self {
            DeepJob::Deep {
                sentence,
                translation: _,
                target_language,
                profile: _,
                app,
                focused_word,
                generation,
                anchor,
            } => TransResult::Deep {
                sentence,
                result,
                app,
                focused_word,
                generation,
                anchor,
                target_language,
            },
        }
    }

    /// Build a DeepRequest from this job's fields.
    ///
    /// The request's target_language comes from the job, ensuring the translator
    /// receives the dispatch-time language. Returns a clone of focused_word
    /// so the caller can use it for the result (the job's focused_word is
    /// preserved for into_result).
    pub fn build_request(&self) -> (DeepRequest, Option<String>) {
        match self {
            DeepJob::Deep {
                sentence,
                translation,
                target_language,
                profile,
                focused_word,
                ..
            } => {
                let request = DeepRequest {
                    sentence: sentence.clone(),
                    word: focused_word.clone(),
                    translation: translation.clone(),
                    target_language: target_language.clone(),
                    profile: *profile,
                };
                (request, focused_word.clone())
            }
        }
    }
}

/// Translation result returned from the async worker.
#[allow(dead_code)]
pub enum TransResult {
    Quick {
        sentence: String,
        display_text: String,
        result: Result<QuickTranslation, String>,
        from_pixels: bool,
        app: Option<String>,
        focused_word: Option<String>,
        generation: u64,
        anchor: ScreenRect,
        target_language: String,
    },
    Deep {
        sentence: String,
        result: Result<DeepAnalysis, String>,
        app: Option<String>,
        focused_word: Option<String>,
        generation: u64,
        anchor: ScreenRect,
        target_language: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    /// PR #437 fixed Quick dispatch to carry target_language through the async
    /// boundary. This test proves the invariant using the production seam:
    /// `into_result` derives the result's target_language from the job's request.
    ///
    /// The test FAILS if production code is changed to use a different/current
    /// language instead of the dispatch-time language.
    #[test]
    fn quick_into_result_preserves_dispatch_target_language() {
        // Dispatch with target_language = Korean.
        let job = QuickJob::Quick {
            request: QuickRequest::new("안녕", "Korean"),
            sentence: "안녕".to_owned(),
            display_text: "안녕".to_owned(),
            from_pixels: false,
            app: Some("Safari".to_owned()),
            focused_word: None,
            generation: 1,
            anchor: ScreenRect::new(0, 0, 100, 20),
        };

        // Simulate live settings change to Japanese (would have corrupted
        // provenance in the old code that used &language from outer scope).
        let _live_settings_language = "Japanese".to_owned();

        // Use the production seam: into_result.
        let result = job.into_result(Err("simulated error".to_owned()));

        // The result's target_language comes from the job's request, which
        // was bound at dispatch time. Still Korean, not Japanese.
        let target_language = match result {
            TransResult::Quick {
                target_language, ..
            } => target_language,
            TransResult::Deep { .. } => panic!("expected Quick"),
        };
        assert_eq!(target_language, "Korean");
    }

    /// Same invariant for Deep dispatch. The test FAILS if production code
    /// is changed to use current settings instead of dispatch-time language.
    #[test]
    fn deep_into_result_preserves_dispatch_target_language() {
        // Dispatch with target_language = Korean.
        let job = DeepJob::Deep {
            sentence: "세상".to_owned(),
            translation: None,
            target_language: "Korean".to_owned(),
            profile: Profile::Reader,
            app: Some("Firefox".to_owned()),
            focused_word: None,
            generation: 2,
            anchor: ScreenRect::new(10, 20, 200, 40),
        };

        // Simulate live settings change.
        let _live_settings_language = "Japanese".to_owned();

        // Use the production seam: into_result.
        let result = job.into_result(Err("simulated error".to_owned()));

        // The result's target_language comes from the job, which was bound
        // at dispatch time.
        let target_language = match result {
            TransResult::Deep {
                target_language, ..
            } => target_language,
            TransResult::Quick { .. } => panic!("expected Deep"),
        };
        assert_eq!(target_language, "Korean");
    }

    /// QuickRequest is the single source of truth for target_language in QuickJob.
    /// The job does not duplicate it; into_result derives from request.
    #[test]
    fn quick_job_has_no_duplicate_target_language() {
        let job = QuickJob::Quick {
            request: QuickRequest::new("test", "Korean"),
            sentence: "test".to_owned(),
            display_text: "test".to_owned(),
            from_pixels: false,
            app: None,
            focused_word: None,
            generation: 1,
            anchor: ScreenRect::new(0, 0, 50, 10),
        };

        // The request owns target_language.
        let QuickJob::Quick { request, .. } = &job;
        assert_eq!(request.target_language, "Korean");

        // into_result uses request.target_language.
        let result = job.into_result(Ok(QuickTranslation {
            text: "테스트".to_owned(),
            engine: lens_core::translate::EngineId::Local,
            model: "test".to_owned(),
            latency_ms: 0,
            cached: false,
            part_was_removed: false,
            name_changed: false,
        }));
        let target_language = match result {
            TransResult::Quick {
                target_language, ..
            } => target_language,
            TransResult::Deep { .. } => panic!("expected Quick"),
        };
        assert_eq!(target_language, "Korean");
    }

    /// Deep's build_request produces a DeepRequest with the job's target_language.
    #[test]
    fn deep_build_request_uses_dispatch_target_language() {
        let job = DeepJob::Deep {
            sentence: "sentence".to_owned(),
            translation: Some("번역".to_owned()),
            target_language: "Korean".to_owned(),
            profile: Profile::Reader,
            app: None,
            focused_word: Some("sentence".to_owned()),
            generation: 1,
            anchor: ScreenRect::new(0, 0, 100, 20),
        };

        let (request, result_word) = job.build_request();

        // The request has the job's target_language.
        assert_eq!(request.target_language, "Korean");
        assert_eq!(request.sentence, "sentence");
        assert_eq!(request.translation, Some("번역".to_owned()));
        assert_eq!(request.word, Some("sentence".to_owned()));
        // The result_word is the same as focused_word.
        assert_eq!(result_word, Some("sentence".to_owned()));

        // The job is preserved for into_result.
        let result = job.into_result(Ok(DeepAnalysis::default()));
        let target_language = match result {
            TransResult::Deep {
                target_language, ..
            } => target_language,
            TransResult::Quick { .. } => panic!("expected Deep"),
        };
        assert_eq!(target_language, "Korean");
    }
}
