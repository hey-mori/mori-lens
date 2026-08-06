//! Screen geometry, in one coordinate space.
//!
//! Everything here is **physical screen pixels**, the space both Windows and
//! macOS report bounding rectangles in. Conversion to logical units happens at
//! the one boundary that needs it, never in the middle of a calculation. Mixing
//! the two is the standard way multi-monitor code goes wrong, and it goes wrong
//! silently: the numbers stay plausible.
//!
//! Coordinates are signed on purpose. A monitor placed left of or above the
//! primary produces negative origins, and unsigned types turn that into a
//! wrapped positive number that looks like a coordinate.

/// A point in physical screen pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScreenPoint {
    pub x: i32,
    pub y: i32,
}

impl ScreenPoint {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    /// Euclidean distance, in pixels.
    pub fn distance_to(self, other: Self) -> f64 {
        let dx = f64::from(other.x - self.x);
        let dy = f64::from(other.y - self.y);
        dx.hypot(dy)
    }
}

/// A rectangle in physical screen pixels, described by its origin and size.
///
/// Origin-plus-size rather than left/top/right/bottom because the platform APIs
/// disagree about which they return, and picking one here means the conversion
/// is written once per adapter instead of being re-derived at each use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl ScreenRect {
    pub const fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// From left, top, right, bottom. The right and bottom edges are exclusive.
    pub fn from_ltrb(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        Self {
            x: left,
            y: top,
            width: (right - left).max(0),
            height: (bottom - top).max(0),
        }
    }

    pub const fn right(self) -> i32 {
        self.x + self.width
    }

    pub const fn bottom(self) -> i32 {
        self.y + self.height
    }

    /// A rectangle with no area. UI Automation returns these for text that is
    /// scrolled out of view, which is a real state and not an error: the range
    /// exists, the user cannot see it.
    pub const fn is_empty(self) -> bool {
        self.width <= 0 || self.height <= 0
    }

    /// Whether these two rectangles share any pixel.
    ///
    /// Half-open on the right and bottom, like every other rectangle here: two
    /// rectangles that merely touch along an edge do not overlap, because the
    /// pixel at `right` belongs to whatever is next to it.
    pub fn overlaps(self, other: Self) -> bool {
        self.x < other.right()
            && other.x < self.right()
            && self.y < other.bottom()
            && other.y < self.bottom()
    }

    pub fn contains(self, point: ScreenPoint) -> bool {
        point.x >= self.x && point.x < self.right() && point.y >= self.y && point.y < self.bottom()
    }

    pub fn center(self) -> ScreenPoint {
        ScreenPoint::new(self.x + self.width / 2, self.y + self.height / 2)
    }

    /// Distance from a point to the nearest edge, or zero if the point is
    /// inside.
    ///
    /// This is how a "nearest text range" result gets sanity checked.
    /// `RangeFromPoint` answers with the nearest text even when the pointer is
    /// in a wide margin, so without a distance check Lens would confidently
    /// explain a sentence the user was not looking at.
    pub fn distance_to(self, point: ScreenPoint) -> f64 {
        if self.contains(point) {
            return 0.0;
        }
        let dx = if point.x < self.x {
            self.x - point.x
        } else if point.x >= self.right() {
            point.x - self.right() + 1
        } else {
            0
        };
        let dy = if point.y < self.y {
            self.y - point.y
        } else if point.y >= self.bottom() {
            point.y - self.bottom() + 1
        } else {
            0
        };
        f64::from(dx).hypot(f64::from(dy))
    }

    /// The smallest rectangle containing both.
    pub fn union(self, other: Self) -> Self {
        if self.is_empty() {
            return other;
        }
        if other.is_empty() {
            return self;
        }
        Self::from_ltrb(
            self.x.min(other.x),
            self.y.min(other.y),
            self.right().max(other.right()),
            self.bottom().max(other.bottom()),
        )
    }
}

/// The union of a set of rectangles, or `None` when the set has no area.
///
/// Text ranges come back as one rectangle per rendered line, so this is how a
/// multi-line range gets a single anchor.
pub fn bounding_box(rects: &[ScreenRect]) -> Option<ScreenRect> {
    rects
        .iter()
        .copied()
        .filter(|rect| !rect.is_empty())
        .reduce(ScreenRect::union)
}

/// The rectangle nearest a point, ignoring empty ones.
pub fn nearest_rect(rects: &[ScreenRect], point: ScreenPoint) -> Option<ScreenRect> {
    rects
        .iter()
        .copied()
        .filter(|rect| !rect.is_empty())
        .min_by(|a, b| {
            a.distance_to(point)
                .partial_cmp(&b.distance_to(point))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
}

/// How far outside a line's own rectangle still counts as on that line, as a
/// multiple of the line's height.
///
/// Half. A line's glyph box is roughly two thirds of its pitch, so the gap
/// between two lines is about half a box: half above and half below closes it
/// exactly, and no further. Anything more would reach past the neighbouring
/// line, and the nearest-line rule would still resolve it correctly, but the
/// margin at the end of a paragraph would start answering too.
pub const LINE_SLACK: f64 = 0.5;

/// Which of these text lines the point is on, if any.
///
/// The index rather than the rectangle, so a caller holding lines and their
/// text can get back to the one it asked about.
///
/// `None` is the honest answer for a pointer in a margin. The accessibility
/// path's equivalent mistake was answering with the nearest text anywhere on
/// the page, and
/// [ADR-0009](../../../docs/decisions/ADR-0009-pointer-distance-gate.md) exists
/// because of it. [`LINE_SLACK`] is what keeps this on the right side of that.
///
/// # Why it is here rather than beside a recogniser
///
/// Two callers ask it. The OCR reader asks which line the pointer is on so it
/// can pick a word, and [`crate::barren`] asks whether the pointer is on any
/// line at all so it can skip a capture. Those have to agree: a point the
/// second calls empty and the first would have read is a card that does not
/// appear. One definition is the only way to promise that, and the rule is
/// arithmetic on rectangles, so it belongs where it can be tested without a
/// screen.
pub fn line_under(lines: &[ScreenRect], point: ScreenPoint) -> Option<usize> {
    let mut best: Option<(f64, usize)> = None;

    for (at, bounds) in lines.iter().enumerate() {
        if bounds.is_empty() {
            continue;
        }
        let slack = f64::from(bounds.height.max(1)) * LINE_SLACK;

        // Horizontal first, and unforgiving beyond one slack: past the end of a
        // line is the margin, and the margin is where answering with the
        // nearest text is the mistake ADR-0009 names.
        let x = f64::from(point.x);
        if x < f64::from(bounds.x) - slack || x > f64::from(bounds.right()) + slack {
            continue;
        }

        let above = f64::from(bounds.y) - f64::from(point.y);
        let below = f64::from(point.y) - f64::from(bounds.bottom());
        let gap = above.max(below).max(0.0);
        if gap > slack {
            continue;
        }
        // Nearest wins, which is what makes the tolerance safe to set wide
        // enough to close the gap between two lines: a point in that gap is
        // inside both tolerances and belongs to the closer line.
        if best.is_none() || best.is_some_and(|(closest, _)| gap < closest) {
            best = Some((gap, at));
        }
    }

    best.map(|(_, at)| at)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distance_is_zero_inside_and_grows_outside() {
        let rect = ScreenRect::new(10, 10, 100, 20);
        assert_eq!(rect.distance_to(ScreenPoint::new(50, 15)), 0.0);
        assert_eq!(rect.distance_to(ScreenPoint::new(5, 15)), 5.0);
        assert_eq!(rect.distance_to(ScreenPoint::new(50, 5)), 5.0);
    }

    #[test]
    fn negative_origin_monitors_are_ordinary_coordinates() {
        // A monitor left of the primary. Unsigned arithmetic would turn this
        // into a rectangle several billion pixels wide.
        let rect = ScreenRect::from_ltrb(-1920, -100, -1820, -80);
        assert_eq!(rect.width, 100);
        assert_eq!(rect.height, 20);
        assert!(rect.contains(ScreenPoint::new(-1900, -90)));
        assert_eq!(rect.distance_to(ScreenPoint::new(-1930, -90)), 10.0);
    }

    #[test]
    fn union_ignores_empty_rects() {
        let a = ScreenRect::new(0, 0, 10, 10);
        let empty = ScreenRect::new(500, 500, 0, 0);
        assert_eq!(a.union(empty), a);
        assert_eq!(empty.union(a), a);
    }

    #[test]
    fn bounding_box_spans_every_line_of_a_range() {
        let lines = [
            ScreenRect::new(10, 10, 200, 18),
            ScreenRect::new(10, 30, 140, 18),
        ];
        let box_ = bounding_box(&lines).expect("two non-empty rects have a union");
        assert_eq!(box_, ScreenRect::from_ltrb(10, 10, 210, 48));
    }

    #[test]
    fn bounding_box_of_scrolled_out_text_is_none() {
        let offscreen = [ScreenRect::new(0, 0, 0, 0)];
        assert_eq!(bounding_box(&offscreen), None);
    }

    #[test]
    fn nearest_rect_picks_the_line_the_pointer_is_on() {
        let lines = [
            ScreenRect::new(10, 10, 200, 18),
            ScreenRect::new(10, 30, 200, 18),
        ];
        let hit = nearest_rect(&lines, ScreenPoint::new(50, 35)).expect("a line is nearest");
        assert_eq!(hit.y, 30);
    }
}

#[cfg(test)]
mod overlap_tests {
    use super::ScreenRect;

    #[test]
    fn two_rectangles_that_share_pixels_overlap() {
        let card = ScreenRect::new(100, 100, 300, 180);
        assert!(card.overlaps(ScreenRect::new(200, 150, 100, 100)));
        assert!(
            card.overlaps(ScreenRect::new(0, 0, 200, 200)),
            "a corner counts"
        );
        assert!(card.overlaps(card), "a rectangle overlaps itself");
    }

    #[test]
    fn two_rectangles_that_only_touch_do_not() {
        // Half-open on the right and bottom, like every other rectangle here:
        // the pixel at `right` belongs to whatever is next to it.
        let card = ScreenRect::new(100, 100, 300, 180);
        assert!(
            !card.overlaps(ScreenRect::new(400, 100, 50, 50)),
            "edge to edge"
        );
        assert!(
            !card.overlaps(ScreenRect::new(100, 280, 50, 50)),
            "top to bottom"
        );
    }

    #[test]
    fn a_band_away_from_the_card_does_not_overlap_it() {
        // The case this exists for: a reader hovering a margin while a card
        // sits somewhere else, with automatic OCR on. The card must stay.
        let card = ScreenRect::new(900, 400, 300, 180);
        let band = ScreenRect::new(100, 700, 760, 180);
        assert!(!card.overlaps(band));
        assert!(!band.overlaps(card), "and the other way round");
    }
}
