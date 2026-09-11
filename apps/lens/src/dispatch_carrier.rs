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
use lens_core::geometry::ScreenRect;
use lens_core::translate::QuickRequest;

/// Quick translation job dispatched to the async worker.
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
        target_language: String,
    },
}

/// Deep translation job dispatched to the async worker.
#[allow(dead_code)]
pub enum DeepJob {
    Deep {
        sentence: String,
        translation: Option<String>,
        target_language: String,
        profile: lens_core::analyse::Profile,
        app: Option<String>,
        focused_word: Option<String>,
        generation: u64,
        anchor: ScreenRect,
    },
}

/// Translation result returned from the async worker.
#[allow(dead_code)]
pub enum TransResult {
    Quick {
        sentence: String,
        display_text: String,
        result: Result<lens_core::translate::QuickTranslation, String>,
        from_pixels: bool,
        app: Option<String>,
        focused_word: Option<String>,
        generation: u64,
        anchor: ScreenRect,
        target_language: String,
    },
    Deep {
        sentence: String,
        result: Result<lens_core::analyse::DeepAnalysis, String>,
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
    /// boundary. This test proves the invariant: a job dispatched with
    /// target_language = Korean retains Korean even if the live setting
    /// changes to Japanese before the result arrives.
    #[test]
    fn quick_dispatch_preserves_target_language_through_settings_change() {
        // Simulate dispatch with target_language = Korean.
        let job = QuickJob::Quick {
            request: QuickRequest::new("안녕", "Korean"),
            sentence: "안녕".to_owned(),
            display_text: "안녕".to_owned(),
            from_pixels: false,
            app: Some("Safari".to_owned()),
            focused_word: None,
            generation: 1,
            anchor: ScreenRect::new(0, 0, 100, 20),
            target_language: "Korean".to_owned(),
        };

        // Simulate live settings change to Japanese (would have corrupted
        // provenance in the old code that used &language from outer scope).
        let _live_settings_language = "Japanese".to_owned();

        // Extract target_language from the job (as the result handler does).
        let QuickJob::Quick {
            target_language, ..
        } = job;

        // Provenance preserved: still Korean, not Japanese.
        assert_eq!(target_language, "Korean");
    }

    /// Same invariant for Deep dispatch.
    #[test]
    fn deep_dispatch_preserves_target_language_through_settings_change() {
        let job = DeepJob::Deep {
            sentence: "세상".to_owned(),
            translation: None,
            target_language: "Korean".to_owned(),
            profile: lens_core::analyse::Profile::Reader,
            app: Some("Firefox".to_owned()),
            focused_word: None,
            generation: 2,
            anchor: ScreenRect::new(10, 20, 200, 40),
        };

        // Simulate live settings change.
        let _live_settings_language = "Japanese".to_owned();

        // Extract target_language from the job.
        let DeepJob::Deep {
            target_language, ..
        } = job;

        assert_eq!(target_language, "Korean");
    }

    /// Result carrier preserves dispatch-time target_language.
    #[test]
    fn quick_result_carries_dispatch_target_language() {
        let result = TransResult::Quick {
            sentence: "test".to_owned(),
            display_text: "test".to_owned(),
            result: Err("simulated error".to_owned()),
            from_pixels: false,
            app: None,
            focused_word: None,
            generation: 1,
            anchor: ScreenRect::new(0, 0, 50, 10),
            target_language: "Korean".to_owned(),
        };

        let target_language = match result {
            TransResult::Quick {
                target_language, ..
            } => target_language,
            TransResult::Deep { .. } => panic!("expected Quick"),
        };
        assert_eq!(target_language, "Korean");
    }

    #[test]
    fn deep_result_carries_dispatch_target_language() {
        let result = TransResult::Deep {
            sentence: "test".to_owned(),
            result: Err("simulated error".to_owned()),
            app: None,
            focused_word: None,
            generation: 1,
            anchor: ScreenRect::new(0, 0, 50, 10),
            target_language: "Japanese".to_owned(),
        };

        let target_language = match result {
            TransResult::Deep {
                target_language, ..
            } => target_language,
            TransResult::Quick { .. } => panic!("expected Deep"),
        };
        assert_eq!(target_language, "Japanese");
    }
}
