//! Image capture and thumbnail generation.

use std::io::Cursor;
use std::path::Path;

use image::codecs::jpeg::JpegEncoder;
use image::{DynamicImage, GenericImageView, ImageFormat};

use crate::types::{CaptureSource, VisionResult};

/// Maximum thumbnail dimension (width or height).
const MAX_THUMBNAIL_SIZE: u32 = 512;

/// JPEG quality for thumbnails.
const THUMBNAIL_QUALITY: u8 = 85;

/// Load an image from a file path.
pub fn capture_from_file(path: &str) -> VisionResult<(DynamicImage, CaptureSource)> {
    let img = image::open(path)?;
    let source = CaptureSource::File {
        path: path.to_string(),
    };
    Ok((img, source))
}

/// Load an image from base64-encoded data.
pub fn capture_from_base64(data: &str, mime: &str) -> VisionResult<(DynamicImage, CaptureSource)> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|e| crate::types::VisionError::InvalidInput(format!("Invalid base64: {e}")))?;

    let format = match mime {
        "image/png" => Some(ImageFormat::Png),
        "image/jpeg" | "image/jpg" => Some(ImageFormat::Jpeg),
        "image/webp" => Some(ImageFormat::WebP),
        "image/gif" => Some(ImageFormat::Gif),
        _ => None,
    };

    let img = if let Some(fmt) = format {
        image::load_from_memory_with_format(&bytes, fmt)?
    } else {
        image::load_from_memory(&bytes)?
    };

    let source = CaptureSource::Base64 {
        mime: mime.to_string(),
    };
    Ok((img, source))
}

/// Generate a JPEG thumbnail, preserving aspect ratio, max 512x512.
pub fn generate_thumbnail(img: &DynamicImage) -> Vec<u8> {
    let (w, h) = img.dimensions();

    let thumb = if w > MAX_THUMBNAIL_SIZE || h > MAX_THUMBNAIL_SIZE {
        img.resize(
            MAX_THUMBNAIL_SIZE,
            MAX_THUMBNAIL_SIZE,
            image::imageops::FilterType::Lanczos3,
        )
    } else {
        img.clone()
    };

    let rgb = thumb.to_rgb8();
    let mut buf = Vec::new();
    let mut cursor = Cursor::new(&mut buf);
    let encoder = JpegEncoder::new_with_quality(&mut cursor, THUMBNAIL_QUALITY);
    rgb.write_with_encoder(encoder).unwrap_or_else(|e| {
        tracing::warn!("Failed to encode thumbnail as JPEG: {e}");
    });
    buf
}

/// Check if a file path points to a supported image format.
pub fn is_supported_format(path: &str) -> bool {
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    matches!(
        ext.as_str(),
        "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp" | "tiff" | "tif" | "ico"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_thumbnail_small_image() {
        let img = DynamicImage::new_rgb8(100, 100);
        let thumb = generate_thumbnail(&img);
        assert!(!thumb.is_empty());
    }

    #[test]
    fn test_thumbnail_large_image() {
        let img = DynamicImage::new_rgb8(2000, 1000);
        let thumb = generate_thumbnail(&img);
        assert!(!thumb.is_empty());

        // Verify the thumbnail can be loaded back
        let loaded = image::load_from_memory(&thumb).unwrap();
        let (w, h) = loaded.dimensions();
        assert!(w <= MAX_THUMBNAIL_SIZE);
        assert!(h <= MAX_THUMBNAIL_SIZE);
    }

    #[test]
    fn test_supported_formats() {
        assert!(is_supported_format("test.png"));
        assert!(is_supported_format("test.JPG"));
        assert!(is_supported_format("test.webp"));
        assert!(!is_supported_format("test.txt"));
        assert!(!is_supported_format("test.pdf"));
    }
}
