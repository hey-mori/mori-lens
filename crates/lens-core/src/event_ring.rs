//! Privacy-safe diagnostic event ring.
//!
//! A bounded, in-memory, session-local ring of typed diagnostic events.
//! Dies with the process. No persistence, no disk spill.
//!
//! # What is here and what is not
//!
//! This module answers "what recent abnormal operational events explain why
//! something is not working?" It does NOT answer "what did the user do?"
//! Routine user activity (hover, card, copy, save) is not recorded here.
//!
//! [`crate::tally::Tally`] answers the aggregate question ("how often did
//! ordinary acquisition succeed/refuse this session?"). The event ring
//! complements it with rare operational incidents.
//!
//! # Privacy by construction
//!
//! Event payloads contain only enums, numbers, and booleans. No free-form
//! `String` fields exist that could carry captured text, translations,
//! analysis, file paths, or error messages. This is enforced by the type
//! system, not by hoping callers redact strings correctly.

use serde::Serialize;
use std::collections::VecDeque;
use std::time::Instant;

/// What component reported the event.
///
/// Closed enum so the UI can map tokens to copy without parsing strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticComponent {
    /// The main runtime loop.
    Runtime,
    /// Text acquisition (accessibility or OCR).
    Acquisition,
    /// Quick translation engine.
    Quick,
    /// Deep analysis engine.
    Deep,
    /// OCR subsystem.
    Ocr,
    /// Library (storage backing).
    Library,
    /// Settings reload / validation.
    Settings,
    /// Global shortcut registration.
    Shortcut,
    /// Health check probe.
    HealthCheck,
}

/// What kind of event occurred.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticEventKind {
    /// A component failed.
    Failure,
    /// A component is degraded but partially functional.
    Degraded,
    /// A component recovered from a previous failure/degraded state.
    Recovered,
    /// An explicit health check result.
    HealthCheck,
}

/// Typed failure classification.
///
/// Closed enum with no free-form string field. Maps provider transport
/// failures, library issues, settings problems, and shortcut conflicts
/// to diagnostic tokens the UI can render as product copy.
///
/// Does not carry raw error text because provider errors can contain URLs,
/// paths, response bodies, and tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticFailureClass {
    /// Could not connect to the provider endpoint.
    ConnectionFailed,
    /// Provider did not respond within the time budget.
    Timeout,
    /// Provider returned an HTTP 5xx or other server error.
    HttpServer,
    /// Provider returned HTTP 4xx (client error).
    HttpClient,
    /// The configured model is not installed on the provider.
    ModelMissing,
    /// Provider answered but the response could not be parsed.
    Malformed,
    /// Provider answered but inference did not produce usable output.
    InferenceFailed,
    /// Settings file could not be parsed or validated.
    SettingsInvalid,
    /// Library fell back to in-memory storage.
    LibraryDegraded,
    /// A global shortcut could not be registered.
    ShortcutUnavailable,
    /// A background worker stopped unexpectedly.
    WorkerStopped,
}

/// A single diagnostic event in the ring.
///
/// Privacy-safe by construction: all fields are enums, numbers, or
/// booleans. No captured text, no file paths, no raw error messages.
#[derive(Debug, Clone, Serialize)]
pub struct DiagnosticEvent {
    /// Monotonically increasing sequence number.
    pub seq: u64,
    /// Milliseconds since process start (monotonic, not wall-clock).
    pub elapsed_ms: u64,
    /// What component reported this.
    pub component: DiagnosticComponent,
    /// What kind of event.
    pub kind: DiagnosticEventKind,
    /// Typed failure class (present for Failure/Degraded events).
    pub failure_class: Option<DiagnosticFailureClass>,
    /// Number of coalesced occurrences. Always >= 1.
    pub count: u32,
    /// When the first occurrence of a coalesced run was seen (elapsed_ms).
    pub first_seen_ms: u64,
}

/// Default ring capacity. Chosen to hold enough context for diagnosis
/// without unbounded memory growth. 128 events at ~56 bytes each is ~7KB.
const DEFAULT_CAPACITY: usize = 128;

/// Bounded in-memory ring of diagnostic events.
///
/// Thread-safe via external synchronisation (`Arc<Mutex<...>>`). The ring
/// itself is not thread-safe, matching the repository's pattern of
/// `Shared*` type aliases.
///
/// When full, the oldest entry is evicted first (FIFO). Consecutive
/// equivalent events (same component + kind + failure_class) are
/// coalesced by incrementing the count rather than adding a new entry.
#[derive(Debug)]
pub struct DiagnosticEventRing {
    events: VecDeque<DiagnosticEvent>,
    capacity: usize,
    next_seq: u64,
    start_time: Instant,
}

impl DiagnosticEventRing {
    /// Create a new ring with the default capacity.
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_CAPACITY)
    }

    /// Create a new ring with an explicit capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            events: VecDeque::with_capacity(capacity),
            capacity,
            next_seq: 1,
            start_time: Instant::now(),
        }
    }

    /// Record an event. If the most recent event has the same component,
    /// kind, and failure class, increment its count instead of appending.
    ///
    /// Returns the sequence number assigned to the event.
    pub fn record(
        &mut self,
        component: DiagnosticComponent,
        kind: DiagnosticEventKind,
        failure_class: Option<DiagnosticFailureClass>,
    ) -> u64 {
        let elapsed = self.elapsed_ms();

        // Coalesce with the most recent event if equivalent.
        if let Some(last) = self.events.back_mut() {
            if last.component == component
                && last.kind == kind
                && last.failure_class == failure_class
            {
                last.count += 1;
                return last.seq;
            }
        }

        let seq = self.next_seq;
        self.next_seq += 1;

        let event = DiagnosticEvent {
            seq,
            elapsed_ms: elapsed,
            component,
            kind,
            failure_class,
            count: 1,
            first_seen_ms: elapsed,
        };

        if self.events.len() >= self.capacity {
            self.events.pop_front();
        }
        self.events.push_back(event);

        seq
    }

    /// Current events in insertion order (oldest first).
    pub fn events(&self) -> impl Iterator<Item = &DiagnosticEvent> {
        self.events.iter()
    }

    /// Number of events currently in the ring.
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// Whether the ring is empty.
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Maximum number of events the ring can hold.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Clear all events.
    pub fn clear(&mut self) {
        self.events.clear();
    }

    /// Return a snapshot of current events as owned data.
    ///
    /// The caller can hold this snapshot without holding the ring's lock.
    /// Events are returned oldest-first.
    pub fn snapshot(&self) -> Vec<DiagnosticEvent> {
        self.events.iter().cloned().collect()
    }

    /// Milliseconds since the ring was created.
    fn elapsed_ms(&self) -> u64 {
        self.start_time.elapsed().as_millis() as u64
    }
}

impl Default for DiagnosticEventRing {
    fn default() -> Self {
        Self::new()
    }
}

/// Map a [`crate::translate::TranslateError`] to a diagnostic failure class.
///
/// This is the bridge between the product-level error taxonomy and the
/// diagnostic event ring. The mapping is intentionally lossy: the ring
/// records only the category, not the error message text (which could
/// contain captured content, file paths, or provider URLs).
pub fn failure_class_from_translate_error(
    error: &crate::translate::TranslateError,
) -> DiagnosticFailureClass {
    use crate::translate::TranslateError;
    match error {
        TranslateError::Unavailable => DiagnosticFailureClass::ConnectionFailed,
        TranslateError::Timeout => DiagnosticFailureClass::Timeout,
        TranslateError::ModelMissing(_) => DiagnosticFailureClass::ModelMissing,
        TranslateError::Malformed => DiagnosticFailureClass::Malformed,
        TranslateError::NotTranslated => DiagnosticFailureClass::InferenceFailed,
        TranslateError::AlreadyReadable => DiagnosticFailureClass::InferenceFailed,
        TranslateError::Stale => DiagnosticFailureClass::InferenceFailed,
        TranslateError::NotWorthTranslating => DiagnosticFailureClass::InferenceFailed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_ring_is_empty() {
        let ring = DiagnosticEventRing::new();
        assert!(ring.is_empty());
        assert_eq!(ring.len(), 0);
        assert_eq!(ring.capacity(), DEFAULT_CAPACITY);
    }

    #[test]
    fn record_adds_an_event() {
        let mut ring = DiagnosticEventRing::new();
        let seq = ring.record(
            DiagnosticComponent::Quick,
            DiagnosticEventKind::Failure,
            Some(DiagnosticFailureClass::Timeout),
        );
        assert_eq!(seq, 1);
        assert_eq!(ring.len(), 1);

        let event = ring.events().next().unwrap();
        assert_eq!(event.component, DiagnosticComponent::Quick);
        assert_eq!(event.kind, DiagnosticEventKind::Failure);
        assert_eq!(event.failure_class, Some(DiagnosticFailureClass::Timeout));
        assert_eq!(event.count, 1);
    }

    #[test]
    fn coalescing_increments_count_for_equivalent_events() {
        let mut ring = DiagnosticEventRing::new();
        ring.record(
            DiagnosticComponent::Quick,
            DiagnosticEventKind::Failure,
            Some(DiagnosticFailureClass::Timeout),
        );
        ring.record(
            DiagnosticComponent::Quick,
            DiagnosticEventKind::Failure,
            Some(DiagnosticFailureClass::Timeout),
        );
        ring.record(
            DiagnosticComponent::Quick,
            DiagnosticEventKind::Failure,
            Some(DiagnosticFailureClass::Timeout),
        );

        assert_eq!(ring.len(), 1, "coalesced into one event");
        let event = ring.events().next().unwrap();
        assert_eq!(event.count, 3);
    }

    #[test]
    fn different_failure_class_starts_new_event() {
        let mut ring = DiagnosticEventRing::new();
        ring.record(
            DiagnosticComponent::Quick,
            DiagnosticEventKind::Failure,
            Some(DiagnosticFailureClass::Timeout),
        );
        ring.record(
            DiagnosticComponent::Quick,
            DiagnosticEventKind::Failure,
            Some(DiagnosticFailureClass::ConnectionFailed),
        );

        assert_eq!(ring.len(), 2, "different failure class = separate events");
    }

    #[test]
    fn different_component_starts_new_event() {
        let mut ring = DiagnosticEventRing::new();
        ring.record(
            DiagnosticComponent::Quick,
            DiagnosticEventKind::Failure,
            Some(DiagnosticFailureClass::Timeout),
        );
        ring.record(
            DiagnosticComponent::Deep,
            DiagnosticEventKind::Failure,
            Some(DiagnosticFailureClass::Timeout),
        );

        assert_eq!(ring.len(), 2, "different component = separate events");
    }

    #[test]
    fn fifo_eviction_when_full() {
        let mut ring = DiagnosticEventRing::with_capacity(3);
        ring.record(
            DiagnosticComponent::Quick,
            DiagnosticEventKind::Failure,
            Some(DiagnosticFailureClass::Timeout),
        );
        ring.record(
            DiagnosticComponent::Deep,
            DiagnosticEventKind::Failure,
            Some(DiagnosticFailureClass::ConnectionFailed),
        );
        ring.record(
            DiagnosticComponent::Library,
            DiagnosticEventKind::Degraded,
            Some(DiagnosticFailureClass::LibraryDegraded),
        );
        assert_eq!(ring.len(), 3);

        // Fourth event evicts the first.
        ring.record(
            DiagnosticComponent::Ocr,
            DiagnosticEventKind::Failure,
            Some(DiagnosticFailureClass::Malformed),
        );
        assert_eq!(ring.len(), 3);

        let components: Vec<_> = ring.events().map(|e| e.component).collect();
        assert_eq!(
            components,
            vec![
                DiagnosticComponent::Deep,
                DiagnosticComponent::Library,
                DiagnosticComponent::Ocr,
            ],
            "oldest (Quick) was evicted"
        );
    }

    #[test]
    fn sequence_numbers_are_monotonic() {
        let mut ring = DiagnosticEventRing::new();
        let s1 = ring.record(
            DiagnosticComponent::Quick,
            DiagnosticEventKind::Failure,
            Some(DiagnosticFailureClass::Timeout),
        );
        let s2 = ring.record(
            DiagnosticComponent::Deep,
            DiagnosticEventKind::Recovered,
            None,
        );
        assert!(s2 > s1, "sequence numbers increase");
    }

    #[test]
    fn coalesced_event_keeps_original_sequence_number() {
        let mut ring = DiagnosticEventRing::new();
        let first_seq = ring.record(
            DiagnosticComponent::Quick,
            DiagnosticEventKind::Failure,
            Some(DiagnosticFailureClass::Timeout),
        );
        let coalesced_seq = ring.record(
            DiagnosticComponent::Quick,
            DiagnosticEventKind::Failure,
            Some(DiagnosticFailureClass::Timeout),
        );
        assert_eq!(
            first_seq, coalesced_seq,
            "coalesced event returns the same seq"
        );
    }

    #[test]
    fn elapsed_time_is_non_decreasing() {
        let mut ring = DiagnosticEventRing::new();
        ring.record(
            DiagnosticComponent::Quick,
            DiagnosticEventKind::Failure,
            Some(DiagnosticFailureClass::Timeout),
        );
        ring.record(
            DiagnosticComponent::Deep,
            DiagnosticEventKind::Failure,
            Some(DiagnosticFailureClass::ConnectionFailed),
        );

        let times: Vec<u64> = ring.events().map(|e| e.elapsed_ms).collect();
        assert!(times[0] <= times[1], "times are non-decreasing");
    }

    #[test]
    fn clear_empties_the_ring() {
        let mut ring = DiagnosticEventRing::new();
        ring.record(
            DiagnosticComponent::Quick,
            DiagnosticEventKind::Failure,
            Some(DiagnosticFailureClass::Timeout),
        );
        assert!(!ring.is_empty());
        ring.clear();
        assert!(ring.is_empty());
    }

    #[test]
    fn recovered_event_has_no_failure_class() {
        let mut ring = DiagnosticEventRing::new();
        ring.record(
            DiagnosticComponent::Quick,
            DiagnosticEventKind::Recovered,
            None,
        );
        let event = ring.events().next().unwrap();
        assert_eq!(event.kind, DiagnosticEventKind::Recovered);
        assert_eq!(event.failure_class, None);
    }

    /// Privacy: the event type carries no String field that could
    /// accidentally hold captured text, file paths, or error messages.
    /// This test verifies the serialization contains only expected tokens.
    #[test]
    fn serialized_event_contains_only_safe_tokens() {
        let mut ring = DiagnosticEventRing::new();
        ring.record(
            DiagnosticComponent::Quick,
            DiagnosticEventKind::Failure,
            Some(DiagnosticFailureClass::Timeout),
        );

        let event = ring.events().next().unwrap();
        let json = serde_json::to_string(event).unwrap();

        // Should contain only enum tokens and numbers.
        assert!(json.contains("\"component\":\"quick\""));
        assert!(json.contains("\"kind\":\"failure\""));
        assert!(json.contains("\"failure_class\":\"timeout\""));
        assert!(json.contains("\"seq\":1"));
        assert!(json.contains("\"count\":1"));
    }

    /// Privacy: no canary strings can appear in serialized events because
    /// the type system has no field to carry them.
    #[test]
    fn serialized_events_exclude_all_canary_strings() {
        let canaries = [
            "SUPER_SECRET_ORIGINAL_CANARY",
            "SUPER_SECRET_TRANSLATION_CANARY",
            "SUPER_SECRET_ANALYSIS_CANARY",
            "SUPER_SECRET_NOTE_CANARY",
            "SUPER_SECRET_HISTORY_CANARY",
            "SUPER_SECRET_RECENT_CANARY",
            "SUPER_SECRET_WINDOW_TITLE_CANARY",
            "SUPER_SECRET_APP_NAME_CANARY",
            "SUPER_SECRET_PATH_CANARY",
            "SUPER_SECRET_TOKEN_CANARY",
            "SUPER_SECRET_ERROR_BODY_CANARY",
        ];

        let mut ring = DiagnosticEventRing::new();
        // Record every kind of event.
        ring.record(
            DiagnosticComponent::Quick,
            DiagnosticEventKind::Failure,
            Some(DiagnosticFailureClass::Timeout),
        );
        ring.record(
            DiagnosticComponent::Library,
            DiagnosticEventKind::Degraded,
            Some(DiagnosticFailureClass::LibraryDegraded),
        );
        ring.record(
            DiagnosticComponent::HealthCheck,
            DiagnosticEventKind::HealthCheck,
            None,
        );

        for event in ring.events() {
            let json = serde_json::to_string(event).unwrap();
            for canary in &canaries {
                assert!(
                    !json.contains(canary),
                    "event serialization contains canary: {canary}"
                );
            }
        }
    }
}
