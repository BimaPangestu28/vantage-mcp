//! macOS `ScreenCapturer` backed by `xcap`.
//!
//! Captures the monitor whose frame contains the requested region's
//! top-left corner, then crops to the exact region in monitor-local
//! coordinates.

use vantage_core::{Bounds, CaptureError, RgbaImage, ScreenCapturer};
use xcap::Monitor;

pub struct MacScreenCapturer;

impl MacScreenCapturer {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MacScreenCapturer {
    fn default() -> Self {
        Self::new()
    }
}

impl ScreenCapturer for MacScreenCapturer {
    fn capture_region(&self, bounds: Bounds) -> Result<RgbaImage, CaptureError> {
        if bounds.width == 0 || bounds.height == 0 {
            return Err(CaptureError::InvalidBounds(bounds));
        }
        let monitors = Monitor::all().map_err(|e| classify_capture_error(&e))?;
        // Pick the monitor whose frame contains the region's top-left corner.
        let monitor = monitors
            .into_iter()
            .find(|m| {
                let (mx, my) = (m.x().unwrap_or(0), m.y().unwrap_or(0));
                let (mw, mh) = (
                    m.width().unwrap_or(0) as i32,
                    m.height().unwrap_or(0) as i32,
                );
                bounds.x >= mx && bounds.y >= my && bounds.x < mx + mw && bounds.y < my + mh
            })
            .ok_or(CaptureError::InvalidBounds(bounds))?;

        let mx = monitor.x().unwrap_or(0);
        let my = monitor.y().unwrap_or(0);
        // `Monitor::x/y/width/height` (from `CGDisplayBounds`) are in POINTS
        // (logical coordinates), but `capture_image` (CGWindowListCreateImage)
        // returns a buffer at native DEVICE-PIXEL resolution. On a 2x Retina
        // display that buffer is 2x the point dimensions, so the requested
        // region (also in points, same Quartz space as window bounds) must be
        // scaled into pixel space before it is used to index the pixel buffer.
        let scale = monitor.scale_factor().unwrap_or(1.0);
        let shot = monitor
            .capture_image()
            .map_err(|e| classify_capture_error(&e))?; // image::RgbaImage

        let (px_x, px_y, crop_w, crop_h) =
            crop_rect(bounds, mx, my, scale, shot.width(), shot.height())
                .ok_or(CaptureError::InvalidBounds(bounds))?;
        let cropped = image::imageops::crop_imm(&shot, px_x, px_y, crop_w, crop_h).to_image();
        Ok(RgbaImage {
            width: crop_w,
            height: crop_h,
            pixels: cropped.into_raw(),
        })
    }
}

/// Computes the pixel-space crop rectangle `(x, y, width, height)` for a
/// point-space `bounds` region on a monitor whose origin is
/// `(monitor_x, monitor_y)` (also in points) and whose captured pixel buffer
/// is `scale`x the point dimensions and sized `shot_w` x `shot_h` pixels.
///
/// The region is first translated into monitor-local point coordinates
/// (clamping negative offsets to the monitor's origin), then scaled into
/// pixel space, then clamped against the actual pixel buffer so the caller
/// never over-reads it.
///
/// Returns `None` when `bounds` is zero-sized or the resulting crop would be
/// empty (e.g. the region lies entirely outside the captured buffer).
fn crop_rect(
    bounds: Bounds,
    monitor_x: i32,
    monitor_y: i32,
    scale: f32,
    shot_w: u32,
    shot_h: u32,
) -> Option<(u32, u32, u32, u32)> {
    if bounds.width == 0 || bounds.height == 0 {
        return None;
    }
    // Guard against a nonsensical scale factor (xcap failure fallback is
    // handled by the caller via `unwrap_or(1.0)`, but be defensive here too).
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };

    // Region origin relative to the monitor, in points. A region whose
    // origin lies left of/above the monitor is clamped to the monitor's
    // origin (mirrors the previous behavior).
    let local_x_pt = (bounds.x as i64 - monitor_x as i64).max(0);
    let local_y_pt = (bounds.y as i64 - monitor_y as i64).max(0);

    // Point -> pixel space.
    let px_x = point_to_pixel(local_x_pt, scale);
    let px_y = point_to_pixel(local_y_pt, scale);
    let px_w = point_to_pixel(bounds.width as i64, scale);
    let px_h = point_to_pixel(bounds.height as i64, scale);

    // Clamp against the actual pixel buffer so we never over-read it.
    let px_x = px_x.min(shot_w);
    let px_y = px_y.min(shot_h);
    let crop_w = px_w.min(shot_w.saturating_sub(px_x));
    let crop_h = px_h.min(shot_h.saturating_sub(px_y));

    if crop_w == 0 || crop_h == 0 {
        None
    } else {
        Some((px_x, px_y, crop_w, crop_h))
    }
}

/// Converts a non-negative points value to a pixel-space `u32` by scaling
/// and rounding to the nearest pixel, saturating on overflow/NaN.
fn point_to_pixel(points: i64, scale: f32) -> u32 {
    let pixels = points as f64 * scale as f64;
    if pixels.is_finite() {
        pixels.round().clamp(0.0, u32::MAX as f64) as u32
    } else {
        0
    }
}

/// xcap does not expose a distinct "screen recording permission denied"
/// error variant on macOS (`XCapError` has no such case); it only surfaces
/// `Objc2CoreGraphicsCGError`, `Error(String)`, `InvalidCaptureRegion`, etc.
/// We heuristically classify by message content, and otherwise fall back to
/// `Internal`. In practice, when Screen Recording access is missing on
/// current macOS versions, `CGWindowListCreateImage` tends to return black
/// or empty imagery rather than a distinguishable error, so this heuristic
/// is a best-effort classification and end-to-end verification (empty/black
/// captures) is the ultimate backstop.
fn classify_capture_error(err: &xcap::XCapError) -> CaptureError {
    let msg = err.to_string().to_lowercase();
    if msg.contains("permission") || msg.contains("denied") || msg.contains("not authorized") {
        CaptureError::ScreenRecordingPermissionDenied
    } else {
        CaptureError::Internal(format!("capture: {err}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// scale = 1.0: point space and pixel space coincide, so the crop
    /// rectangle should equal the requested region unchanged.
    #[test]
    fn crop_rect_scale_1_basic_crop() {
        let bounds = Bounds {
            x: 0,
            y: 0,
            width: 64,
            height: 64,
        };
        let result = crop_rect(bounds, 0, 0, 1.0, 1920, 1080);
        assert_eq!(result, Some((0, 0, 64, 64)));
    }

    /// scale = 2.0 (Retina): a point-space region maps to a pixel rect at
    /// twice the offset and twice the extent. This is the core Retina bug:
    /// without scaling, offsets computed in points would be used directly
    /// as pixel offsets, cropping the wrong (top-left-shifted) sub-region.
    #[test]
    fn crop_rect_scale_2_maps_region_and_offset() {
        let bounds = Bounds {
            x: 10,
            y: 20,
            width: 64,
            height: 64,
        };
        let result = crop_rect(bounds, 0, 0, 2.0, 3840, 2160);
        assert_eq!(result, Some((20, 40, 128, 128)));
    }

    /// A monitor with a non-zero origin (e.g. a secondary display to the
    /// right of the primary at x=1440) must have its origin subtracted
    /// before scaling, exercising the global -> local -> pixel translation.
    #[test]
    fn crop_rect_nonzero_monitor_origin() {
        let bounds = Bounds {
            x: 1450,
            y: 20,
            width: 50,
            height: 60,
        };
        let result = crop_rect(bounds, 1440, 0, 1.0, 1920, 1080);
        assert_eq!(result, Some((10, 20, 50, 60)));
    }

    /// When the requested region extends past the captured pixel buffer,
    /// the crop must clamp to the buffer's actual extent rather than
    /// over-reading it.
    #[test]
    fn crop_rect_clamps_when_region_extends_past_shot() {
        let bounds = Bounds {
            x: 90,
            y: 90,
            width: 50,
            height: 50,
        };
        let result = crop_rect(bounds, 0, 0, 1.0, 100, 100);
        assert_eq!(result, Some((90, 90, 10, 10)));
    }

    /// Clamping also applies after scaling: a region that maps to a pixel
    /// rect overlapping the buffer only partially is clamped, not dropped.
    #[test]
    fn crop_rect_clamps_after_scaling() {
        let bounds = Bounds {
            x: 40,
            y: 40,
            width: 40,
            height: 40,
        };
        let result = crop_rect(bounds, 0, 0, 2.0, 100, 100);
        assert_eq!(result, Some((80, 80, 20, 20)));
    }

    /// A region that, once scaled, falls entirely outside the pixel buffer
    /// yields an empty crop, which the caller maps to `InvalidBounds`.
    #[test]
    fn crop_rect_returns_none_when_entirely_outside_shot() {
        let bounds = Bounds {
            x: 60,
            y: 60,
            width: 40,
            height: 40,
        };
        let result = crop_rect(bounds, 0, 0, 2.0, 100, 100);
        assert_eq!(result, None);
    }

    /// Zero-size regions are rejected up front, independent of scale.
    #[test]
    fn crop_rect_rejects_zero_size_bounds() {
        let bounds = Bounds {
            x: 0,
            y: 0,
            width: 0,
            height: 10,
        };
        assert_eq!(crop_rect(bounds, 0, 0, 1.0, 100, 100), None);
    }

    /// A non-finite or non-positive scale factor (xcap failure) falls back
    /// to 1.0 rather than corrupting the crop math.
    #[test]
    fn crop_rect_falls_back_to_scale_1_on_invalid_scale() {
        let bounds = Bounds {
            x: 0,
            y: 0,
            width: 64,
            height: 64,
        };
        assert_eq!(
            crop_rect(bounds, 0, 0, f32::NAN, 1920, 1080),
            Some((0, 0, 64, 64))
        );
        assert_eq!(
            crop_rect(bounds, 0, 0, 0.0, 1920, 1080),
            Some((0, 0, 64, 64))
        );
        assert_eq!(
            crop_rect(bounds, 0, 0, -2.0, 1920, 1080),
            Some((0, 0, 64, 64))
        );
    }
}
