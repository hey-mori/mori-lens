//! The first-run lesson, wired in as an acquirer.
//!
//! [`lens_core::welcome`] chooses the sentence and
//! [`lens_platform_windows::welcome`] puts it on screen. This is the join: an
//! acquirer that answers for points inside the welcome window and delegates
//! everything else, so a staged hover travels the ordinary path and the loop
//! above needs no idea that any of this exists.
//!
//! # Why a decorator rather than a branch in the loop
//!
//! "Where did these characters come from" is acquisition's question. Putting it
//! anywhere else means the dwell, the distance gate, the language decision, the
//! cache, and the card each grow a special case for a window that exists for
//! one minute of a product's life. Here, they grow none: the first card a new
//! reader sees is produced by exactly the code that produces every card after
//! it, which is also what makes it evidence that this machine can run the
//! product at all.

use lens_core::acquire::{
    AcquireContext, AcquireError, AcquisitionSource, AppIdentity, ExpandedUnit, ProviderInfo,
    TextAcquirer, TextTarget,
};
use lens_core::geometry::{ScreenPoint, ScreenRect};
use lens_platform_windows::welcome::Placement;

/// Tries the lesson, then the real thing.
pub struct TaughtFirst {
    placement: Placement,
    then: Box<dyn TextAcquirer>,
}

impl TaughtFirst {
    pub fn new(placement: Placement, then: Box<dyn TextAcquirer>) -> Self {
        Self { placement, then }
    }
}

impl TextAcquirer for TaughtFirst {
    fn acquire(
        &self,
        point: ScreenPoint,
        context: &AcquireContext,
    ) -> Result<TextTarget, AcquireError> {
        match self.placement.line_under(point) {
            Some((rect, line)) => Ok(target_for(line, rect, point)),
            // Not the welcome window, or the welcome window with something on
            // top of it. Either way this has nothing to say, and saying
            // nothing here is what keeps the lesson from ever answering for
            // somebody else's text.
            None => self.then.acquire(point, context),
        }
    }
}

/// Build the target the rest of the product would have built.
///
/// # The focus is an estimate, and only one thing depends on it
///
/// A real provider is asked which word is at a point. Nothing here can ask, so
/// the pointer's position across the rectangle is scaled onto the sentence and
/// rounded to a character boundary. That decides which word `Ctrl+Alt+D` would
/// explain and nothing else: the sentence, the card, and the translation are
/// the same wherever inside the line the reader stopped.
fn target_for(line: &'static str, rect: ScreenRect, point: ScreenPoint) -> TextTarget {
    TextTarget {
        app: AppIdentity {
            // Named, because the tray, the logs, and the per-application rules
            // all key off this and an empty name would make the lesson look
            // like an application the reader has never heard of.
            process_name: Some("lens".to_owned()),
            window_title: None,
        },
        source: AcquisitionSource::Lesson,
        provider: ProviderInfo {
            control_id: None,
            woke_accessibility: false,
            expanded_unit: ExpandedUnit::classify(line),
        },
        segmented: lens_core::text::segment(line, focus_in(line, rect, point)),
        word_bounds: vec![rect],
        context_bounds: vec![rect],
    }
}

/// Scale the pointer's position across the line onto a character boundary.
fn focus_in(line: &str, rect: ScreenRect, point: ScreenPoint) -> usize {
    if rect.width <= 0 || line.is_empty() {
        return 0;
    }
    let across = (point.x - rect.x).clamp(0, rect.width) as f64 / rect.width as f64;
    let wanted = (line.len() as f64 * across) as usize;
    // Rounding down to a boundary rather than up: `wanted` can land on
    // `line.len()`, and every byte index below the length has a boundary at or
    // beneath it while the length itself has nothing above it.
    (0..=wanted.min(line.len()))
        .rev()
        .find(|index| line.is_char_boundary(*index))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LINE: &str = "이 문장 위에 포인터를 잠시 멈춰 두세요.";
    const LATIN: &str = "Let the pointer rest on this sentence for a moment.";

    fn rect() -> ScreenRect {
        ScreenRect::new(100, 200, 400, 20)
    }

    /// The focus must always be a character boundary.
    ///
    /// `segment` slices the string at it. On a Korean lesson every character is
    /// three bytes, so a naive scaling lands mid-character two times in three
    /// and panics in a release binary, on the first hover a new reader ever
    /// performs.
    #[test]
    fn every_position_across_a_korean_line_lands_on_a_boundary() {
        for x in 90..=510 {
            let focus = focus_in(LINE, rect(), ScreenPoint::new(x, 210));
            assert!(
                LINE.is_char_boundary(focus),
                "x={x} gave {focus}, which splits a character",
            );
        }
    }

    /// And the whole target must survive being built at any of them.
    #[test]
    fn a_target_can_be_built_anywhere_on_the_line() {
        for x in 90..=510 {
            let target = target_for(LINE, rect(), ScreenPoint::new(x, 210));
            assert_eq!(target.segmented.sentence_text().trim(), LINE);
            assert_eq!(target.source, AcquisitionSource::Lesson);
        }
    }

    #[test]
    fn the_focus_follows_the_pointer_across_the_line() {
        let left = focus_in(LATIN, rect(), ScreenPoint::new(110, 210));
        let right = focus_in(LATIN, rect(), ScreenPoint::new(490, 210));
        assert!(left < right, "{left} is not left of {right}");
    }

    /// A pointer outside the rectangle cannot index past the end.
    ///
    /// `line_under` allows half a line of slack either side, so a point beyond
    /// the right edge does reach here.
    #[test]
    fn a_pointer_past_the_edge_is_clamped() {
        let focus = focus_in(LATIN, rect(), ScreenPoint::new(9_000, 210));
        assert!(focus <= LATIN.len());
        assert!(LATIN.is_char_boundary(focus));
        let before = focus_in(LATIN, rect(), ScreenPoint::new(-9_000, 210));
        assert_eq!(before, 0);
    }

    /// A zero-width rectangle is not a division by zero.
    #[test]
    fn a_line_with_no_width_still_answers() {
        let flat = ScreenRect::new(100, 200, 0, 20);
        assert_eq!(focus_in(LATIN, flat, ScreenPoint::new(100, 210)), 0);
    }

    /// The lesson never answers for a point it was not asked about.
    ///
    /// The delegate is what reads the reader's own screen, so a lesson that
    /// swallowed a miss would silently replace every acquisition on the desktop
    /// with a sentence this repository wrote.
    #[test]
    fn a_point_outside_the_window_goes_to_the_real_acquirer() {
        struct Delegated;
        impl TextAcquirer for Delegated {
            fn acquire(
                &self,
                _point: ScreenPoint,
                _context: &AcquireContext,
            ) -> Result<TextTarget, AcquireError> {
                Err(AcquireError::Unsupported)
            }
        }
        // An empty placement owns no point on any desktop, which is the same
        // shape as a welcome window the reader has closed.
        let taught = TaughtFirst::new(
            Placement {
                window_id: 0,
                lines: Vec::new(),
            },
            Box::new(Delegated),
        );
        let answer = taught.acquire(
            ScreenPoint::new(10, 10),
            &AcquireContext::new(lens_core::hover::Generation(0)),
        );
        assert!(
            matches!(answer, Err(AcquireError::Unsupported)),
            "{answer:?}"
        );
    }
}
