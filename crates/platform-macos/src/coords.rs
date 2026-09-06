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

    /// Find the display containing a Lens physical pixel point.
    ///
    /// Returns `None` if the point is not on any known display.
    pub fn display_at_screen_point(&self, point: ScreenPoint) -> Option<&DisplayInfo> {
        for display in &self.displays {
            let b = &display.bounds_points;
            let s = display.scale;
            let px_left = (b.origin.x * s).round() as i32;
            let px_top = (b.origin.y * s).round() as i32;
            let px_right = ((b.origin.x + b.size.width) * s).round() as i32;
            let px_bottom = ((b.origin.y + b.size.height) * s).round() as i32;
            if point.x >= px_left && point.x < px_right && point.y >= px_top && point.y < px_bottom
            {
                return Some(display);
            }
        }
        None
    }

    /// Find the scale factor for the display containing a Lens physical pixel
    /// point.
    ///
    /// Iterates through displays and converts each display's logical-point
    /// bounds to physical pixels using that display's own scale factor, then
    /// checks containment. This avoids the circular dependency of needing the
    /// scale to convert the point but needing the point to find the scale.
    pub fn scale_at_screen_point(&self, point: ScreenPoint) -> f64 {
        for display in &self.displays {
            let b = &display.bounds_points;
            let s = display.scale;
            // Convert display bounds from logical points to physical pixels.
            let px_left = (b.origin.x * s).round() as i32;
            let px_top = (b.origin.y * s).round() as i32;
            let px_right = ((b.origin.x + b.size.width) * s).round() as i32;
            let px_bottom = ((b.origin.y + b.size.height) * s).round() as i32;
            if point.x >= px_left && point.x < px_right && point.y >= px_top && point.y < px_bottom
            {
                return s;
            }
        }
        // Fallback: if the point is not on any known display, use 2.0 as a
        // safe Retina default.
        2.0
    }
}

/// Compute the display-local crop rectangle for a capture request.
///
/// Given a capture region in global logical points and a display topology,
/// returns:
/// - The display containing the capture origin (or the first display as fallback)
/// - The crop rectangle in display-local logical points, clipped to display bounds
/// - The output pixel dimensions at the display's native scale
///
/// Returns `None` if the clipped crop has zero or negative area.
pub fn compute_crop_rect(
    topology: &DisplayTopology,
    global_x: f64,
    global_y: f64,
    width: f64,
    height: f64,
) -> Option<(DisplayInfo, ffi::CGRect, i32, i32)> {
    if width <= 0.0 || height <= 0.0 {
        return None;
    }

    let display = topology
        .displays
        .iter()
        .find(|d| {
            let b = &d.bounds_points;
            global_x >= b.origin.x
                && global_x < b.origin.x + b.size.width
                && global_y >= b.origin.y
                && global_y < b.origin.y + b.size.height
        })
        .or_else(|| topology.displays.first())?;

    let b = &display.bounds_points;

    let mut crop_x = global_x - b.origin.x;
    let mut crop_y = global_y - b.origin.y;
    let mut crop_w = width;
    let mut crop_h = height;

    if crop_x < 0.0 {
        crop_w += crop_x;
        crop_x = 0.0;
    }
    if crop_y < 0.0 {
        crop_h += crop_y;
        crop_y = 0.0;
    }
    if crop_x + crop_w > b.size.width {
        crop_w = b.size.width - crop_x;
    }
    if crop_y + crop_h > b.size.height {
        crop_h = b.size.height - crop_y;
    }

    if crop_w <= 0.0 || crop_h <= 0.0 {
        return None;
    }

    let scale = display.scale;
    let out_w = (crop_w * scale).round() as i32;
    let out_h = (crop_h * scale).round() as i32;
    if out_w <= 0 || out_h <= 0 {
        return None;
    }

    let local_rect = ffi::CGRect {
        origin: ffi::CGPoint {
            x: crop_x,
            y: crop_y,
        },
        size: ffi::CGSize {
            width: crop_w,
            height: crop_h,
        },
    };

    Some((*display, local_rect, out_w, out_h))
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

    #[test]
    fn scale_at_screen_point_finds_correct_display_in_mixed_scale_topology() {
        // Primary: Retina 1440x900 logical = 2880x1800 physical.
        // External: 1x 1920x1080 logical = 1920x1080 physical, to the right.
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

        // Physical pixel on the Retina display (middle of primary).
        let on_retina = ScreenPoint::new(1440, 900);
        assert!(
            (topo.scale_at_screen_point(on_retina) - 2.0).abs() < 0.001,
            "center of Retina display should resolve to scale 2.0"
        );

        // Physical pixel on the external 1x display.
        // External starts at logical x=1440, which is physical x=2880.
        let on_external = ScreenPoint::new(3000, 500);
        assert!(
            (topo.scale_at_screen_point(on_external) - 1.0).abs() < 0.001,
            "point on external 1x display should resolve to scale 1.0"
        );

        // Physical pixel near the boundary of the Retina display.
        // Right edge of primary at physical x=2879 (last pixel).
        let near_edge = ScreenPoint::new(2879, 100);
        assert!(
            (topo.scale_at_screen_point(near_edge) - 2.0).abs() < 0.001,
            "last physical pixel on Retina should still be scale 2.0"
        );

        // First physical pixel on the external display.
        let first_external = ScreenPoint::new(2880, 100);
        assert!(
            (topo.scale_at_screen_point(first_external) - 1.0).abs() < 0.001,
            "first physical pixel on external should be scale 1.0"
        );
    }

    #[test]
    fn scale_at_screen_point_negative_display_physical_pixels() {
        // A monitor to the left: logical -1920..0, physical at 1x = -1920..0.
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

        let p = ScreenPoint::new(-500, 300);
        assert!((topo.scale_at_screen_point(p) - 1.0).abs() < 0.001);
    }

    fn dual_monitor_topology() -> DisplayTopology {
        DisplayTopology {
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
        }
    }

    #[test]
    fn display_at_screen_point_finds_primary_retina() {
        let topo = dual_monitor_topology();
        // Center of primary Retina (physical: 1440x900 = middle of 2880x1800).
        let d = topo.display_at_screen_point(ScreenPoint::new(1440, 900));
        assert!(d.is_some());
        assert_eq!(d.unwrap().id, 1);
    }

    #[test]
    fn display_at_screen_point_finds_external_1x() {
        let topo = dual_monitor_topology();
        // Physical pixel on external display: logical x=1440 → physical x=2880.
        let d = topo.display_at_screen_point(ScreenPoint::new(3000, 500));
        assert!(d.is_some());
        assert_eq!(d.unwrap().id, 2);
    }

    #[test]
    fn display_at_screen_point_off_display_returns_none() {
        let topo = dual_monitor_topology();
        // Far below both displays.
        let d = topo.display_at_screen_point(ScreenPoint::new(500, 99999));
        assert!(d.is_none());
    }

    #[test]
    fn crop_rect_on_primary_retina() {
        let topo = dual_monitor_topology();
        // Capture band centered at logical (500, 400) on primary, 600x120 points.
        let result = compute_crop_rect(&topo, 500.0, 400.0, 600.0, 120.0);
        assert!(result.is_some());
        let (display, rect, out_w, out_h) = result.unwrap();
        assert_eq!(display.id, 1);
        // Display-local: same as global since primary origin is (0,0).
        assert!((rect.origin.x - 500.0).abs() < 0.001);
        assert!((rect.origin.y - 400.0).abs() < 0.001);
        assert!((rect.size.width - 600.0).abs() < 0.001);
        assert!((rect.size.height - 120.0).abs() < 0.001);
        // Output at 2x scale.
        assert_eq!(out_w, 1200);
        assert_eq!(out_h, 240);
    }

    #[test]
    fn crop_rect_on_external_1x() {
        let topo = dual_monitor_topology();
        // Capture on external display at logical (2000, 500), 600x120 points.
        let result = compute_crop_rect(&topo, 2000.0, 500.0, 600.0, 120.0);
        assert!(result.is_some());
        let (display, rect, out_w, out_h) = result.unwrap();
        assert_eq!(display.id, 2);
        // Display-local: subtract external origin (1440, 0).
        assert!((rect.origin.x - 560.0).abs() < 0.001);
        assert!((rect.origin.y - 500.0).abs() < 0.001);
        // Output at 1x scale.
        assert_eq!(out_w, 600);
        assert_eq!(out_h, 120);
    }

    #[test]
    fn crop_rect_clipped_to_display_boundary() {
        let topo = dual_monitor_topology();
        // Capture near right edge of primary: 500x120 starting at x=1200.
        // Primary is 1440 wide, so crop extends to 1700 → clipped to 1440.
        let result = compute_crop_rect(&topo, 1200.0, 400.0, 500.0, 120.0);
        assert!(result.is_some());
        let (_, rect, out_w, _) = result.unwrap();
        assert!((rect.origin.x - 1200.0).abs() < 0.001);
        // Clipped width: 1440 - 1200 = 240.
        assert!((rect.size.width - 240.0).abs() < 0.001);
        assert_eq!(out_w, 480); // 240 * 2x
    }

    #[test]
    fn crop_rect_outside_all_displays_returns_none() {
        let topo = dual_monitor_topology();
        // Far outside any display.
        let result = compute_crop_rect(&topo, 99999.0, 99999.0, 600.0, 120.0);
        // Falls back to first display, but crop is entirely outside → clipped to zero.
        assert!(result.is_none());
    }

    #[test]
    fn crop_rect_zero_dimension_returns_none() {
        let topo = dual_monitor_topology();
        assert!(compute_crop_rect(&topo, 500.0, 400.0, 0.0, 120.0).is_none());
        assert!(compute_crop_rect(&topo, 500.0, 400.0, 600.0, 0.0).is_none());
        assert!(compute_crop_rect(&topo, 500.0, 400.0, -1.0, 120.0).is_none());
    }

    #[test]
    fn crop_rect_on_negative_display() {
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
        // Capture at logical (-500, 300) on the left display.
        let result = compute_crop_rect(&topo, -500.0, 300.0, 600.0, 120.0);
        assert!(result.is_some());
        let (display, rect, out_w, out_h) = result.unwrap();
        assert_eq!(display.id, 1);
        // Display-local: -500 - (-1920) = 1420.
        assert!((rect.origin.x - 1420.0).abs() < 0.001);
        assert!((rect.origin.y - 300.0).abs() < 0.001);
        // Crop extends from x=1420 to x=2020, but display is 1920 wide.
        // Clipped to width 500.
        assert!((rect.size.width - 500.0).abs() < 0.001);
        assert_eq!(out_w, 500);
        assert_eq!(out_h, 120);
    }
}
