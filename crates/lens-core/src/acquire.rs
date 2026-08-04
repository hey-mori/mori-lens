//! The contract between the domain and an operating system.
//!
//! One trait, one result type, one closed error enum. A macOS adapter
//! implements this and nothing above it changes
//! (`docs/decisions/ADR-0003-rust-core-platform-adapters.md`).
//!
//! The error enum is the part worth reading twice. Its variants are the
//! product's failure taxonomy, not an error-handling detail: Milestone 1 exists
//! to count them per application, and collapsing them into one "did not work"
//! would destroy the only data that milestone produces.

use crate::geometry::{bounding_box, ScreenPoint, ScreenRect};
use crate::hover::{Generation, TargetFingerprint};
use crate::text::SegmentedText;

/// Where the characters came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcquisitionSource {
    /// The operating system's accessibility tree. The primary path, always
    /// tried first (`docs/decisions/ADR-0002-accessibility-first-acquisition.md`).
    Accessibility,
    /// Optical recognition of a small crop. The fallback, never automatic in
    /// v1. Not implemented yet; the variant exists so the router's match is
    /// exhaustive from the start.
    Ocr,
}

impl AcquisitionSource {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Accessibility => "accessibility",
            Self::Ocr => "ocr",
        }
    }
}

/// Why a protected field was refused.
///
/// Both are refusals. They are distinguished because the second is a
/// *measurement*: if `Unprovable` turns out to be common, the strict rule is
/// costing real coverage and that is a decision to revisit with evidence rather
/// than a bug to patch quietly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtectedReason {
    /// The provider said so. A password box.
    Declared,
    /// The check itself failed, so the field could not be proven safe.
    /// Unreadable means unprovable, and unprovable is treated as secure
    /// (`docs/privacy-model.md` §1).
    Unprovable,
}

/// Every way acquisition can fail to produce text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcquireError {
    /// The provider exposes no text interface at all. Win32 `EDIT` controls,
    /// canvas surfaces, games, video.
    Unsupported,
    /// The read succeeded and returned nothing.
    Empty,
    /// The element vanished mid-call, which happens constantly in a live UI and
    /// is a normal outcome rather than an exception.
    ElementGone,
    /// A password or otherwise protected field.
    Protected(ProtectedReason),
    /// The target application did not answer in time.
    Timeout,
    /// The pointer moved before this finished; the result is unwanted.
    Stale,
    /// The accessibility layer itself is unreachable. A permission or policy
    /// problem, not a property of the application under the pointer.
    Unavailable,
}

impl AcquireError {
    /// The stable identifier used in probe output, logs, and the compatibility
    /// matrix. Changing one of these invalidates recorded runs, so do not.
    pub const fn reason(self) -> &'static str {
        match self {
            Self::Unsupported => "unsupported",
            Self::Empty => "empty",
            Self::ElementGone => "element-gone",
            Self::Protected(ProtectedReason::Declared) => "protected",
            Self::Protected(ProtectedReason::Unprovable) => "protected-unprovable",
            Self::Timeout => "timeout",
            Self::Stale => "stale",
            Self::Unavailable => "unavailable",
        }
    }

    /// Whether the OCR fallback could plausibly repair this.
    ///
    /// `Protected` is never repairable, and that is the point: OCR would read a
    /// password field's rendered characters, so routing a refusal to OCR would
    /// turn a privacy guarantee into a privacy hole.
    pub const fn ocr_can_help(self) -> bool {
        match self {
            Self::Unsupported | Self::Empty | Self::ElementGone => true,
            Self::Protected(_) | Self::Timeout | Self::Stale | Self::Unavailable => false,
        }
    }
}

impl std::fmt::Display for AcquireError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.reason())
    }
}

impl std::error::Error for AcquireError {}

/// Who owns the window under the pointer.
///
/// Every field is optional and an unread field is `None`, never a guess. The
/// probe prints these and an invented source is exactly what a measurement
/// instrument must not produce.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AppIdentity {
    /// The executable file stem, for example `chrome`. The closest thing to an
    /// application name observable without reading version resources.
    pub process_name: Option<String>,
    pub window_title: Option<String>,
}

impl AppIdentity {
    /// A stable short hash, for logs.
    ///
    /// Logs identify applications by hash so a log file does not enumerate what
    /// the user runs (`docs/privacy-model.md` §3). The window title is excluded
    /// because it routinely contains a document name.
    pub fn log_id(&self) -> String {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.process_name.hash(&mut hasher);
        format!("{:06x}", hasher.finish() & 0xff_ffff)
    }
}

/// Adapter-defined detail about the provider that answered, for attributing a
/// failure to the provider rather than to our code.
///
/// `control_id` is opaque on purpose: on Windows it is the UI Automation
/// control type, and the core must not learn what those numbers mean
/// (`docs/decisions/ADR-0003-rust-core-platform-adapters.md`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProviderInfo {
    pub control_id: Option<i32>,
    /// Whether the adapter had to wake the accessibility tree before the read
    /// worked. Chromium and Electron build no tree until an assistive client
    /// asks (`docs/research/inherited-from-mori.md` §2).
    pub woke_accessibility: bool,
}

/// What was read, where it was, and how it was obtained.
#[derive(Debug, Clone)]
pub struct TextTarget {
    pub app: AppIdentity,
    pub source: AcquisitionSource,
    pub provider: ProviderInfo,
    pub segmented: SegmentedText,
    /// One rectangle per rendered line of the word range.
    pub word_bounds: Vec<ScreenRect>,
    /// One rectangle per rendered line of the context range.
    pub context_bounds: Vec<ScreenRect>,
}

impl TextTarget {
    /// Identifies this target for deduplication. The sentence, not the word:
    /// nudging the pointer along one sentence is one target, not eight.
    pub fn fingerprint(&self) -> TargetFingerprint {
        TargetFingerprint::of(
            self.app.process_name.as_deref(),
            self.segmented.sentence_text(),
        )
    }

    /// Where an overlay should anchor, or `None` when the text has no visible
    /// rectangle because it is scrolled out of view.
    pub fn anchor(&self) -> Option<ScreenRect> {
        bounding_box(&self.word_bounds).or_else(|| bounding_box(&self.context_bounds))
    }

    /// How far the resolved word is from where the user actually pointed.
    ///
    /// `RangeFromPoint` answers with the *nearest* text even when the pointer is
    /// in a wide margin, so a large distance means Lens is about to explain a
    /// sentence the user was not looking at. The threshold belongs to the
    /// caller; M1 measures the distribution before one is chosen.
    pub fn pointer_distance(&self, point: ScreenPoint) -> Option<f64> {
        self.nearest_rect_to(point)
            .map(|rect| rect.distance_to(point))
    }

    /// The resolved rectangle nearest a point, or `None` when nothing this
    /// target resolved is currently visible.
    ///
    /// Exposed alongside [`Self::pointer_distance`] so a caller can ask *where*
    /// the resolved text is, not only how far. The two must agree, which is why
    /// the distance is derived from this rather than computed separately: a
    /// distance measured against one rectangle and a direction against another
    /// would describe a geometry that does not exist.
    pub fn nearest_rect_to(&self, point: ScreenPoint) -> Option<ScreenRect> {
        crate::geometry::nearest_rect(&self.word_bounds, point)
            .or_else(|| crate::geometry::nearest_rect(&self.context_bounds, point))
    }
}

/// What the caller wants from one acquisition.
#[derive(Debug, Clone, Copy)]
pub struct AcquireContext {
    /// Checked by the caller before anything is rendered or recorded.
    pub generation: Generation,
    /// Applied by the provider where possible, so an accidental whole-document
    /// range never allocates the whole document.
    pub max_context_chars: usize,
    /// Whether the adapter may send a wake message to a cold accessibility
    /// tree. It costs real milliseconds, so a caller in a hurry can decline
    /// (`docs/research/inherited-from-mori.md` §2).
    pub allow_accessibility_wake: bool,
}

impl AcquireContext {
    pub fn new(generation: Generation) -> Self {
        Self {
            generation,
            max_context_chars: 2_000,
            allow_accessibility_wake: true,
        }
    }
}

/// Read the text under a screen point.
///
/// Implementations must not block for long: a call into a wedged foreign
/// application has to time out rather than stall the pointer loop.
pub trait TextAcquirer {
    fn acquire(
        &self,
        point: ScreenPoint,
        context: &AcquireContext,
    ) -> Result<TextTarget, AcquireError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ocr_is_never_offered_a_protected_field() {
        // If this ever flips, a password field's rendered characters become
        // readable through the fallback.
        assert!(!AcquireError::Protected(ProtectedReason::Declared).ocr_can_help());
        assert!(!AcquireError::Protected(ProtectedReason::Unprovable).ocr_can_help());
    }

    #[test]
    fn ocr_is_offered_the_failures_it_can_actually_repair() {
        assert!(AcquireError::Unsupported.ocr_can_help());
        assert!(AcquireError::Empty.ocr_can_help());
        assert!(AcquireError::ElementGone.ocr_can_help());
    }

    #[test]
    fn ocr_is_not_offered_failures_it_cannot_repair() {
        assert!(!AcquireError::Timeout.ocr_can_help());
        assert!(!AcquireError::Stale.ocr_can_help());
        assert!(!AcquireError::Unavailable.ocr_can_help());
    }

    #[test]
    fn every_failure_has_a_distinct_recorded_reason() {
        let reasons = [
            AcquireError::Unsupported,
            AcquireError::Empty,
            AcquireError::ElementGone,
            AcquireError::Protected(ProtectedReason::Declared),
            AcquireError::Protected(ProtectedReason::Unprovable),
            AcquireError::Timeout,
            AcquireError::Stale,
            AcquireError::Unavailable,
        ]
        .map(AcquireError::reason);

        let unique: std::collections::HashSet<_> = reasons.iter().collect();
        assert_eq!(unique.len(), reasons.len(), "two failures share a reason");
    }

    #[test]
    fn the_log_id_does_not_contain_the_window_title() {
        // The title routinely carries a document name, which is the user's
        // content and must not reach a log line.
        let with_title = AppIdentity {
            process_name: Some("chrome".into()),
            window_title: Some("Quarterly results, confidential".into()),
        };
        let without = AppIdentity {
            process_name: Some("chrome".into()),
            window_title: None,
        };
        assert_eq!(with_title.log_id(), without.log_id());
    }

    #[test]
    fn a_different_process_gets_a_different_log_id() {
        let chrome = AppIdentity {
            process_name: Some("chrome".into()),
            ..Default::default()
        };
        let code = AppIdentity {
            process_name: Some("code".into()),
            ..Default::default()
        };
        assert_ne!(chrome.log_id(), code.log_id());
    }
}
