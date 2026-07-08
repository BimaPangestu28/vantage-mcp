use base64::Engine;
use image::{ImageBuffer, ImageEncoder, Rgba};
use vantage_core::{CaptureError, RgbaImage};

pub const DEFAULT_MAX_DIMENSION: u32 = 1024;

/// Downscale so the largest side is <= `max_dim`, preserving aspect ratio.
/// Returns the input unchanged when it already fits.
pub fn downscale(input: &RgbaImage, max_dim: u32) -> RgbaImage {
    let longest = input.width.max(input.height);
    if max_dim == 0 || longest <= max_dim {
        return input.clone();
    }
    let scale = max_dim as f32 / longest as f32;
    let new_w = (input.width as f32 * scale).round().max(1.0) as u32;
    let new_h = (input.height as f32 * scale).round().max(1.0) as u32;
    let buf: ImageBuffer<Rgba<u8>, Vec<u8>> =
        ImageBuffer::from_raw(input.width, input.height, input.pixels.clone())
            .expect("valid rgba buffer");
    let resized =
        image::imageops::resize(&buf, new_w, new_h, image::imageops::FilterType::Triangle);
    RgbaImage {
        width: new_w,
        height: new_h,
        pixels: resized.into_raw(),
    }
}

/// Encode an RGBA image as a base64 PNG string.
pub fn rgba_to_base64_png(input: &RgbaImage) -> Result<String, CaptureError> {
    let buf: ImageBuffer<Rgba<u8>, Vec<u8>> =
        ImageBuffer::from_raw(input.width, input.height, input.pixels.clone())
            .ok_or_else(|| CaptureError::Internal("invalid rgba buffer".into()))?;
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(
            buf.as_raw(),
            input.width,
            input.height,
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|e| CaptureError::Internal(format!("png encode: {e}")))?;
    Ok(base64::engine::general_purpose::STANDARD.encode(png))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: u32, h: u32) -> RgbaImage {
        RgbaImage {
            width: w,
            height: h,
            pixels: vec![255u8; (w * h * 4) as usize],
        }
    }

    #[test]
    fn downscale_is_noop_when_within_bound() {
        let img = solid(800, 600);
        let out = downscale(&img, 1024);
        assert_eq!((out.width, out.height), (800, 600));
    }

    #[test]
    fn downscale_caps_longest_side_and_keeps_aspect() {
        let img = solid(2000, 1000);
        let out = downscale(&img, 1000);
        assert_eq!(out.width, 1000);
        assert_eq!(out.height, 500);
        assert_eq!(out.pixels.len() as u32, out.width * out.height * 4);
    }

    #[test]
    fn png_roundtrips_dimensions() {
        let b64 = rgba_to_base64_png(&solid(4, 4)).unwrap();
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .unwrap();
        let decoded = image::load_from_memory(&bytes).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (4, 4));
    }
}
