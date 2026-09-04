//! Coordinate conversion between Lens physical pixels and macOS logical points.
//!
//! macOS Accessibility and CoreGraphics work in **logical points** with the
//! origin at the top-left of the primary display. Lens works in **physical
//! screen pixels** with the same origin.
//!
//! The conversion between them is a division/multiplication by the display's
//! scale factor (e.g. 2.0 on Retina). This module handles that conversion,
//! including multi-display topologies where different displays may have
//! different scale factors.
//!
//! Y-axis: both Lens and AX use top-left origin, so no Y-flip is needed.
//! CoreGraphics *event* coordinates use bottom-left origin, but the AX APIs
//! we call (`AXUIElementCopyElementAtPosition`, `AXBoundsForRange`) all use
//! top-left. The flip is irrelevant for our path.

use lens_core::geometry::{ScreenPoint, ScreenRect};

use crate::ffi;

/// Information about a single display in the system.
#[derive(Debug, Clone, Copy)]
pub struct DisplayInfo {
    /// Display ID (CGDirectDisplayID).
    pub id: u32,
    /// Bounds in logical points, top-left origin, global coordinate space.
    pub bounds_points: ffi::CGRect,
    /// Scale factor (backing pixels per point). 2.0 on Retina, 1.0 on
    /// external non-Retina.
    pub scale: f64,
    /// Whether this is the main display.
    pub is_main: bool,
}

/// Snapshot of the current display topology.
#[derive(Debug, Clone)]
pub struct DisplayTopology {
    pub displays: Vec<DisplayInfo>,
}

impl DisplayTopology {
    /// Capture the current display configuration.
    pub fn capture() -> Self {
        let mut ids = [0u32; 16];
        let mut count = 0u32;
        // SAFETY: CGGetActiveDisplayList writes up to 16 display IDs into our
        // stack buffer. We check the error code.
        let err = unsafe { ffi::CGGetActiveDisplayList(16, ids.as_mut_ptr(), &mut count) };
        if err != ffi::K_CG_ERROR_SUCCESS {
            count = 0;
        }

        let mut displays = Vec::with_capacity(count as usize);
        for &id in ids.iter().take(count as usize) {
            // SAFETY: These functions take a valid display ID from the list we
            // just obtained. They are pure queries with no side effects.
            unsafe {
                let bounds = ffi::CGDisplayBounds(id);
                let px_w = ffi::CGDisplayPixelsWide(id);
                let is_main = ffi::CGDisplayIsMain(id) != 0;

                let scale = if bounds.size.width > 0.0 {
                    px_w as f64 / bounds.size.width
                } else {
                    1.0
                };

                displays.push(DisplayInfo {
                    id,
                    bounds_points: bounds,
                    scale,
                    is_main,
                });
            }
        }

        Self { displays }
    }

    /// Find the scale factor for the display containing a point in logical
    /// coordinates (top-left origin, points).
    pub fn scale_at_point(&self, point: ffi::CGPoint) -> f64 {
        for display in &self.displays {
            let b = &display.bounds_points;
            if point.x >= b.origin.x
                && point.x < b.origin.x + b.size.width
                && point.y >= b.origin.y
                && point.y < b.origin.y + b.size.height
            {
                return display.scale;
            }
        }
        // Fallback: if the point is not on any known display (e.g. a display
        // was disconnected between capture and query), use 2.0 as a safe
        // Retina default rather than 1.0, because producing coordinates that
        // are half the real size is worse than producing ones that are double.
        2.0
    }

    /// Find the scale factor for the display containing a Lens physical pixel
    /// point.
    pub fn scale_at_screen_point(&self, point: ScreenPoint) -> f64 {
        let cg = screen_point_to_cg(point, 1.0);
        self.scale_at_point(cg)
    }
}

/// Convert a Lens physical pixel point to a CoreGraphics point (logical
/// points, top-left origin) using the given scale factor.
pub fn screen_point_to_cg(point: ScreenPoint, scale: f64) -> ffi::CGPoint {
    let scale = if scale > 0.0 { scale } else { 1.0 };
    ffi::CGPoint {
        x: point.x as f64 / scale,
        y: point.y as f64 / scale,
    }
}

/// Convert a CoreGraphics point (logical points, top-left origin) to a Lens
/// physical pixel point using the given scale factor.
pub fn cg_to_screen_point(point: ffi::CGPoint, scale: f64) -> ScreenPoint {
    let scale = if scale > 0.0 { scale } else { 1.0 };
    ScreenPoint::new(
        (point.x * scale).round() as i32,
        (point.y * scale).round() as i32,
    )
}

/// Convert an AX position (CGPoint in logical points) and size to a Lens
/// `ScreenRect` in physical pixels.
pub fn ax_rect_to_screen(origin: ffi::CGPoint, size: ffi::CGSize, scale: f64) -> ScreenRect {
    let scale = if scale > 0.0 { scale } else { 1.0 };
    ScreenRect::new(
        (origin.x * scale).round() as i32,
        (origin.y * scale).round() as i32,
        (size.width * scale).round() as i32,
        (size.height * scale).round() as i32,
    )
}

/// Convert a CGRect (logical points) to a Lens `ScreenRect` (physical pixels).
pub fn cg_rect_to_screen(rect: ffi::CGRect, scale: f64) -> ScreenRect {
    ax_rect_to_screen(rect.origin, rect.size, scale)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_at_2x() {
        let original = ScreenPoint::new(1920, 1080);
        let cg = screen_point_to_cg(original, 2.0);
        let back = cg_to_screen_point(cg, 2.0);
        assert_eq!(back, original);
    }

    #[test]
    fn round_trip_at_1x() {
        let original = ScreenPoint::new(-500, 300);
        let cg = screen_point_to_cg(original, 1.0);
        let back = cg_to_screen_point(cg, 1.0);
        assert_eq!(back, original);
    }

    #[test]
    fn screen_point_to_cg_divides_by_scale() {
        let p = ScreenPoint::new(200, 400);
        let cg = screen_point_to_cg(p, 2.0);
        assert!((cg.x - 100.0).abs() < 0.001);
        assert!((cg.y - 200.0).abs() < 0.001);
    }

    #[test]
    fn cg_to_screen_point_multiplies_by_scale() {
        let cg = ffi::CGPoint { x: 100.0, y: 200.0 };
        let p = cg_to_screen_point(cg, 2.0);
        assert_eq!(p, ScreenPoint::new(200, 400));
    }

    #[test]
    fn negative_coordinates_survive_conversion() {
        // A monitor left of the primary.
        let p = ScreenPoint::new(-1920, -100);
        let cg = screen_point_to_cg(p, 2.0);
        assert!((cg.x - (-960.0)).abs() < 0.001);
        assert!((cg.y - (-50.0)).abs() < 0.001);
        let back = cg_to_screen_point(cg, 2.0);
        assert_eq!(back, p);
    }

    #[test]
    fn ax_rect_to_screen_at_retina() {
        let origin = ffi::CGPoint { x: 100.0, y: 50.0 };
        let size = ffi::CGSize {
            width: 200.0,
            height: 18.0,
        };
        let rect = ax_rect_to_screen(origin, size, 2.0);
        assert_eq!(rect, ScreenRect::new(200, 100, 400, 36));
    }

    #[test]
    fn ax_rect_to_screen_at_1x() {
        let origin = ffi::CGPoint { x: 100.0, y: 50.0 };
        let size = ffi::CGSize {
            width: 200.0,
            height: 18.0,
        };
        let rect = ax_rect_to_screen(origin, size, 1.0);
        assert_eq!(rect, ScreenRect::new(100, 50, 200, 18));
    }

    #[test]
    fn zero_scale_falls_back_to_one() {
        let p = ScreenPoint::new(100, 200);
        let cg = screen_point_to_cg(p, 0.0);
        assert!((cg.x - 100.0).abs() < 0.001);
    }

    #[test]
    fn topology_scale_at_point_returns_scale_for_containing_display() {
        let topo = DisplayTopology {
            displays: vec![
                DisplayInfo {
                    id: 1,
                    bounds_points: ffi::CGRect {
                        origin: ffi::CGPoint { x: 0.0, y: 0.0 },
                        size: ffi::CGSize {
                            width: 1440.0,
                            height: 900.0,
                        },
                    },
                    scale: 2.0,
                    is_main: true,
                },
                DisplayInfo {
                    id: 2,
                    bounds_points: ffi::CGRect {
                        origin: ffi::CGPoint { x: 1440.0, y: 0.0 },
                        size: ffi::CGSize {
                            width: 1920.0,
                            height: 1080.0,
                        },
                    },
                    scale: 1.0,
                    is_main: false,
                },
            ],
        };

        let on_main = ffi::CGPoint { x: 500.0, y: 400.0 };
        assert!((topo.scale_at_point(on_main) - 2.0).abs() < 0.001);

        let on_external = ffi::CGPoint {
            x: 2000.0,
            y: 500.0,
        };
        assert!((topo.scale_at_point(on_external) - 1.0).abs() < 0.001);
    }

    #[test]
    fn topology_scale_at_point_negative_display() {
        let topo = DisplayTopology {
            displays: vec![DisplayInfo {
                id: 1,
                bounds_points: ffi::CGRect {
                    origin: ffi::CGPoint { x: -1920.0, y: 0.0 },
                    size: ffi::CGSize {
                        width: 1920.0,
                        height: 1080.0,
                    },
                },
                scale: 1.0,
                is_main: false,
            }],
        };

        let p = ffi::CGPoint {
            x: -500.0,
            y: 300.0,
        };
        assert!((topo.scale_at_point(p) - 1.0).abs() < 0.001);
    }

    #[test]
    fn topology_scale_at_point_off_display_falls_back() {
        let topo = DisplayTopology { displays: vec![] };
        let p = ffi::CGPoint {
            x: 9999.0,
            y: 9999.0,
        };
        // Falls back to 2.0 (Retina default).
        assert!((topo.scale_at_point(p) - 2.0).abs() < 0.001);
    }
}
