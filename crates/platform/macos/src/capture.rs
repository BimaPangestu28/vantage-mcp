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
                let (mw, mh) = (m.width().unwrap_or(0) as i32, m.height().unwrap_or(0) as i32);
                bounds.x >= mx && bounds.y >= my && bounds.x < mx + mw && bounds.y < my + mh
            })
            .ok_or(CaptureError::InvalidBounds(bounds))?;

        let mx = monitor.x().unwrap_or(0);
        let my = monitor.y().unwrap_or(0);
        let shot = monitor.capture_image().map_err(|e| classify_capture_error(&e))?; // image::RgbaImage

        // Crop (region is in global coords; translate to monitor-local).
        let local_x = (bounds.x - mx).max(0) as u32;
        let local_y = (bounds.y - my).max(0) as u32;
        let crop_w = bounds.width.min(shot.width().saturating_sub(local_x));
        let crop_h = bounds.height.min(shot.height().saturating_sub(local_y));
        if crop_w == 0 || crop_h == 0 {
            return Err(CaptureError::InvalidBounds(bounds));
        }
        let cropped = image::imageops::crop_imm(&shot, local_x, local_y, crop_w, crop_h).to_image();
        Ok(RgbaImage { width: crop_w, height: crop_h, pixels: cropped.into_raw() })
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
