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
