//! macOS `TextRecognizer` backed by the Vision framework.
//!
//! Runs `VNRecognizeTextRequest` (accurate level, language correction on)
//! against a `CGImage` built in-process from the caller's RGBA8 buffer. Vision
//! OCR needs no TCC permission and executes synchronously in-process, so this
//! path can be exercised without any user-granted entitlement.
//!
//! All Objective-C / Core Graphics interop is confined to `recognize`. Every
//! fallible step is mapped to [`CaptureError::Internal`]; the FFI path never
//! panics (no `unwrap`/`expect`).

use objc2::AnyThread;
use objc2_core_foundation::CFData;
use objc2_core_graphics::{
    CGBitmapInfo, CGColorRenderingIntent, CGColorSpace, CGDataProvider, CGImage, CGImageAlphaInfo,
};
use objc2_foundation::{NSArray, NSDictionary};
use objc2_vision::{
    VNImageRequestHandler, VNRecognizeTextRequest, VNRequest, VNRequestTextRecognitionLevel,
};
use vantage_core::{CaptureError, RgbaImage, TextRecognizer};

/// Recognizes text in a captured image via Apple's Vision framework.
pub struct MacTextRecognizer;

impl MacTextRecognizer {
    /// Creates a new recognizer. The Vision request is constructed per call,
    /// so this holds no state.
    pub fn new() -> Self {
        Self
    }
}

impl Default for MacTextRecognizer {
    fn default() -> Self {
        Self::new()
    }
}

impl TextRecognizer for MacTextRecognizer {
    fn recognize(&self, image: &RgbaImage) -> Result<String, CaptureError> {
        let cg_image = build_cg_image(image)?;
        run_vision_ocr(&cg_image)
    }
}

/// One recognized line: its recognized string plus the vertical position of
/// its bounding box (Vision normalized coordinates, origin bottom-left, so a
/// larger `y` sits higher on the page).
struct RecognizedLine {
    normalized_top_y: f64,
    text: String,
}

/// Builds an immutable `CGImage` from an RGBA8, row-major buffer.
///
/// The pixel data is copied into a `CFData` (so the returned image owns its
/// backing store independent of `image`), wrapped in a `CGDataProvider`, and
/// tagged sRGB / 8 bpc / 32 bpp with premultiplied-last alpha — matching the
/// `RgbaImage` byte layout.
///
/// # Errors
/// Returns [`CaptureError::Internal`] if the buffer length is inconsistent
/// with `width * height * 4`, if dimensions overflow the platform pointer
/// width, or if any Core Graphics allocation returns null.
fn build_cg_image(
    image: &RgbaImage,
) -> Result<objc2_core_foundation::CFRetained<CGImage>, CaptureError> {
    let width = image.width as usize;
    let height = image.height as usize;

    if image.width == 0 || image.height == 0 {
        return Err(CaptureError::Internal(
            "ocr: image has zero dimension".into(),
        ));
    }

    let bytes_per_pixel = 4usize;
    let expected_len = width
        .checked_mul(height)
        .and_then(|pixels| pixels.checked_mul(bytes_per_pixel))
        .ok_or_else(|| CaptureError::Internal("ocr: image dimensions overflow".into()))?;

    if image.pixels.len() != expected_len {
        return Err(CaptureError::Internal(format!(
            "ocr: pixel buffer length {} does not match {}x{}x4 = {}",
            image.pixels.len(),
            image.width,
            image.height,
            expected_len
        )));
    }

    let bytes_per_row = width * bytes_per_pixel;

    // Copy the pixels into a CFData so the CGImage's backing store is owned by
    // Core Graphics and outlives the borrow of `image`.
    let data = unsafe { CFData::new(None, image.pixels.as_ptr(), image.pixels.len() as isize) }
        .ok_or_else(|| CaptureError::Internal("ocr: CFDataCreate returned null".into()))?;

    let provider = CGDataProvider::with_cf_data(Some(&data)).ok_or_else(|| {
        CaptureError::Internal("ocr: CGDataProviderCreateWithCFData returned null".into())
    })?;

    let color_space = CGColorSpace::new_device_rgb().ok_or_else(|| {
        CaptureError::Internal("ocr: CGColorSpaceCreateDeviceRGB returned null".into())
    })?;

    // RGBA byte order with alpha in the last (least-significant-address) slot:
    // the default byte order already matches an R,G,B,A byte sequence.
    let bitmap_info = CGBitmapInfo(CGImageAlphaInfo::PremultipliedLast.0);

    let cg_image = unsafe {
        CGImage::new(
            width,
            height,
            8,  // bits per component
            32, // bits per pixel
            bytes_per_row,
            Some(&color_space),
            bitmap_info,
            Some(&provider),
            core::ptr::null(), // no custom decode array
            false,             // should_interpolate
            CGColorRenderingIntent::RenderingIntentDefault,
        )
    }
    .ok_or_else(|| CaptureError::Internal("ocr: CGImageCreate returned null".into()))?;

    Ok(cg_image)
}

/// Runs an accurate-level `VNRecognizeTextRequest` over `cg_image` and returns
/// the recognized text, one observation per line, ordered top-to-bottom.
///
/// # Errors
/// Returns [`CaptureError::Internal`] if the request cannot be scheduled or the
/// handler reports a framework error.
fn run_vision_ocr(cg_image: &CGImage) -> Result<String, CaptureError> {
    let request = VNRecognizeTextRequest::new();
    request.setRecognitionLevel(VNRequestTextRecognitionLevel::Accurate);
    request.setUsesLanguageCorrection(true);

    // Empty options dictionary: no camera intrinsics or auxiliary metadata.
    let options = NSDictionary::new();
    let handler = unsafe {
        VNImageRequestHandler::initWithCGImage_options(
            VNImageRequestHandler::alloc(),
            cg_image,
            &options,
        )
    };

    // `performRequests` needs an NSArray<VNRequest>; upcast via deref coercion.
    let request_ref: &VNRequest = &request;
    let requests = NSArray::from_slice(&[request_ref]);

    handler
        .performRequests_error(&requests)
        .map_err(|err| CaptureError::Internal(format!("ocr: Vision perform failed: {err}")))?;

    let observations = match request.results() {
        Some(observations) => observations,
        None => return Ok(String::new()),
    };

    let mut lines: Vec<RecognizedLine> = Vec::with_capacity(observations.len());
    for observation in observations.iter() {
        // Take only the single best candidate for each recognized region.
        let candidates = observation.topCandidates(1);
        let Some(best) = candidates.firstObject() else {
            continue;
        };
        // SAFETY: `boundingBox` reads a normalized CGRect and has no
        // preconditions on a valid observation.
        let bounding_box = unsafe { observation.boundingBox() };
        lines.push(RecognizedLine {
            normalized_top_y: bounding_box.origin.y,
            text: best.string().to_string(),
        });
    }

    // Vision returns observations in an unspecified order. Sort top-to-bottom:
    // higher normalized `y` (origin bottom-left) means higher on the image.
    lines.sort_by(|a, b| {
        b.normalized_top_y
            .partial_cmp(&a.normalized_top_y)
            .unwrap_or(core::cmp::Ordering::Equal)
    });

    let joined = lines
        .into_iter()
        .map(|line| line.text)
        .collect::<Vec<_>>()
        .join("\n");

    Ok(joined)
}
