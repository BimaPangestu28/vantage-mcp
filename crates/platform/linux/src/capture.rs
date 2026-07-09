//! Linux `ScreenCapturer` backed by `xcap` (X11 and Wayland via the portal /
//! PipeWire path xcap selects at runtime).
//!
//! Captures the monitor whose frame contains the requested region's top-left
//! corner, then crops to the exact region. The crop math mirrors the macOS
//! backend: `Monitor::x/y/width/height` are logical coordinates while
//! `capture_image` returns a device-pixel buffer, so on a scaled display
//! (Wayland fractional scaling, GNOME 200%, …) the point-space region is scaled
//! into pixel space before indexing the buffer.

use vantage_core::{Bounds, CaptureError, DisplayInfo, RgbaImage, ScreenCapturer, WindowInfo};
use xcap::Monitor;

pub struct LinuxScreenCapturer;

impl LinuxScreenCapturer {
    pub fn new() -> Self {
        Self
    }
}

impl Default for LinuxScreenCapturer {
    fn default() -> Self {
        Self::new()
    }
}

impl ScreenCapturer for LinuxScreenCapturer {
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

    fn list_displays(&self) -> Result<Vec<DisplayInfo>, CaptureError> {
        let monitors = Monitor::all().map_err(|e| classify_capture_error(&e))?;
        Ok(monitors
            .into_iter()
            .map(|m| DisplayInfo {
                display_id: m.id().unwrap_or(0),
                name: m.name().unwrap_or_default(),
                bounds: Bounds {
                    x: m.x().unwrap_or(0),
                    y: m.y().unwrap_or(0),
                    width: m.width().unwrap_or(0),
                    height: m.height().unwrap_or(0),
                },
                scale_factor: m.scale_factor().unwrap_or(1.0),
                is_primary: m.is_primary().unwrap_or(false),
            })
            .collect())
    }

    fn capture_window(&self, target: &WindowInfo) -> Result<RgbaImage, CaptureError> {
        // Wayland compositors do not permit capturing arbitrary application
        // windows; refuse before touching xcap so we never grab a wrong region.
        let is_wayland = std::env::var("XDG_SESSION_TYPE")
            .map(|v| v.eq_ignore_ascii_case("wayland"))
            .unwrap_or(false)
            || std::env::var("WAYLAND_DISPLAY").is_ok();
        if is_wayland {
            return Err(CaptureError::Unsupported(
                "per-window capture is not available on Wayland; use capture_region with a \
                 display/region, or run under X11"
                    .into(),
            ));
        }
        let windows = xcap::Window::all().map_err(|e| classify_capture_error(&e))?;
        let matches: Vec<xcap::Window> = windows
            .into_iter()
            .filter(|w| {
                w.app_name().map(|a| a == target.app).unwrap_or(false)
                    && w.title().map(|t| t == target.title).unwrap_or(false)
            })
            .collect();
        let win = matches
            .iter()
            .find(|w| {
                w.x().unwrap_or(i32::MIN) == target.bounds.x
                    && w.y().unwrap_or(i32::MIN) == target.bounds.y
            })
            .or_else(|| matches.first())
            .ok_or(CaptureError::WindowNotFound(target.window_id))?;
        let shot = win
            .capture_image()
            .map_err(|e| classify_capture_error(&e))?;
        Ok(RgbaImage {
            width: shot.width(),
            height: shot.height(),
            pixels: shot.into_raw(),
        })
    }
}

/// Computes the pixel-space crop rectangle `(x, y, width, height)` for a
/// point-space `bounds` region on a monitor whose origin is
/// `(monitor_x, monitor_y)` (also in points) and whose captured pixel buffer is
/// `scale`x the point dimensions and sized `shot_w` x `shot_h` pixels.
///
/// The region is translated into monitor-local point coordinates (clamping
/// negative offsets to the origin), scaled into pixel space, then clamped
/// against the actual buffer so the caller never over-reads it. Returns `None`
/// when `bounds` is zero-sized or the resulting crop would be empty.
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
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };

    let local_x_pt = (bounds.x as i64 - monitor_x as i64).max(0);
    let local_y_pt = (bounds.y as i64 - monitor_y as i64).max(0);

    let px_x = point_to_pixel(local_x_pt, scale);
    let px_y = point_to_pixel(local_y_pt, scale);
    let px_w = point_to_pixel(bounds.width as i64, scale);
    let px_h = point_to_pixel(bounds.height as i64, scale);

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

/// Converts a non-negative points value to a pixel-space `u32` by scaling and
/// rounding to the nearest pixel, saturating on overflow/NaN.
fn point_to_pixel(points: i64, scale: f32) -> u32 {
    let pixels = points as f64 * scale as f64;
    if pixels.is_finite() {
        pixels.round().clamp(0.0, u32::MAX as f64) as u32
    } else {
        0
    }
}

/// Map a screen-capture-permission failure distinctly; everything else Internal.
/// On Wayland a denied/cancelled portal request surfaces as a capture error
/// whose message mentions permission/denied; classify those as the dedicated
/// permission variant so the agent gets an actionable error, not a generic one.
fn classify_capture_error(err: &xcap::XCapError) -> CaptureError {
    let msg = err.to_string().to_lowercase();
    if msg.contains("permission")
        || msg.contains("denied")
        || msg.contains("authorized")
        || msg.contains("cancel")
    {
        CaptureError::ScreenRecordingPermissionDenied
    } else {
        CaptureError::Internal(format!("capture: {err}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crop_rect_scale_1_basic_crop() {
        let bounds = Bounds {
            x: 0,
            y: 0,
            width: 64,
            height: 64,
        };
        assert_eq!(
            crop_rect(bounds, 0, 0, 1.0, 1920, 1080),
            Some((0, 0, 64, 64))
        );
    }

    #[test]
    fn crop_rect_scale_2_maps_region_and_offset() {
        let bounds = Bounds {
            x: 10,
            y: 20,
            width: 64,
            height: 64,
        };
        assert_eq!(
            crop_rect(bounds, 0, 0, 2.0, 3840, 2160),
            Some((20, 40, 128, 128))
        );
    }

    #[test]
    fn crop_rect_nonzero_monitor_origin() {
        let bounds = Bounds {
            x: 1450,
            y: 20,
            width: 50,
            height: 60,
        };
        assert_eq!(
            crop_rect(bounds, 1440, 0, 1.0, 1920, 1080),
            Some((10, 20, 50, 60))
        );
    }

    #[test]
    fn crop_rect_clamps_when_region_extends_past_shot() {
        let bounds = Bounds {
            x: 90,
            y: 90,
            width: 50,
            height: 50,
        };
        assert_eq!(
            crop_rect(bounds, 0, 0, 1.0, 100, 100),
            Some((90, 90, 10, 10))
        );
    }

    #[test]
    fn crop_rect_returns_none_when_entirely_outside_shot() {
        let bounds = Bounds {
            x: 60,
            y: 60,
            width: 40,
            height: 40,
        };
        assert_eq!(crop_rect(bounds, 0, 0, 2.0, 100, 100), None);
    }

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
    }
}
