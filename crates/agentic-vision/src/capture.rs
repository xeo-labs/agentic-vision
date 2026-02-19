//! Image capture and thumbnail generation.

use std::io::Cursor;
use std::path::Path;
use std::process::Command;

use image::codecs::jpeg::JpegEncoder;
use image::{DynamicImage, GenericImageView, ImageFormat};

use crate::types::{CaptureSource, Rect, VisionError, VisionResult};

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

// ---------------------------------------------------------------------------
// Screenshot & clipboard capture
// ---------------------------------------------------------------------------

/// RAII guard that removes a temporary file when dropped.
struct TempFileGuard {
    path: std::path::PathBuf,
}

impl Drop for TempFileGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Platform-specific: capture screenshot bytes to a temp file and return the path.
#[cfg(target_os = "macos")]
fn platform_screenshot(temp_path: &Path, region: Option<Rect>) -> VisionResult<()> {
    let mut cmd = Command::new("screencapture");
    cmd.arg("-x"); // silent, no sound

    if let Some(r) = region {
        cmd.arg("-R")
            .arg(format!("{},{},{},{}", r.x, r.y, r.w, r.h));
    }

    cmd.arg(temp_path.to_string_lossy().as_ref());

    let output = cmd
        .output()
        .map_err(|e| VisionError::Capture(format!("Failed to run screencapture: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(VisionError::Capture(format!(
            "screencapture failed (check Screen Recording permission): {stderr}"
        )));
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn platform_screenshot(temp_path: &Path, region: Option<Rect>) -> VisionResult<()> {
    let temp_str = temp_path.to_string_lossy();

    let success = if let Some(r) = region {
        // Region capture: try maim, then import (ImageMagick)
        let geometry = format!("{}x{}+{}+{}", r.w, r.h, r.x, r.y);
        let maim = Command::new("maim")
            .arg("-g")
            .arg(&geometry)
            .arg(temp_str.as_ref())
            .output();
        match maim {
            Ok(o) if o.status.success() => true,
            _ => {
                let import = Command::new("import")
                    .arg("-window")
                    .arg("root")
                    .arg("-crop")
                    .arg(&geometry)
                    .arg(temp_str.as_ref())
                    .output();
                matches!(import, Ok(o) if o.status.success())
            }
        }
    } else {
        // Full-screen: try gnome-screenshot → scrot → maim
        let gnome = Command::new("gnome-screenshot")
            .arg("-f")
            .arg(temp_str.as_ref())
            .output();
        match gnome {
            Ok(o) if o.status.success() => true,
            _ => {
                let scrot = Command::new("scrot").arg(temp_str.as_ref()).output();
                match scrot {
                    Ok(o) if o.status.success() => true,
                    _ => {
                        let maim = Command::new("maim").arg(temp_str.as_ref()).output();
                        matches!(maim, Ok(o) if o.status.success())
                    }
                }
            }
        }
    };

    if !success {
        return Err(VisionError::Capture(
            "No screenshot tool found. Install one of: gnome-screenshot, scrot, maim, or import (ImageMagick).".to_string(),
        ));
    }
    Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn platform_screenshot(_temp_path: &Path, _region: Option<Rect>) -> VisionResult<()> {
    Err(VisionError::Capture(
        "Screenshot capture is not supported on this platform.".to_string(),
    ))
}

/// Platform-specific: read image bytes from the system clipboard.
///
/// macOS clipboard images may be stored as PNG (`PNGf`) or TIFF (`TIFF`).
/// `screencapture -c` writes TIFF, while copy-image-from-browser typically
/// writes PNG. We try PNG first, then fall back to TIFF + `sips` conversion.
#[cfg(target_os = "macos")]
fn platform_clipboard_bytes() -> VisionResult<Vec<u8>> {
    let pid = std::process::id();
    let png_path = std::env::temp_dir().join(format!("avis_clipboard_{pid}.png"));
    let _png_guard = TempFileGuard {
        path: png_path.clone(),
    };

    // --- Attempt 1: read clipboard as PNG directly ---
    let png_script = format!(
        r#"try
    set imgData to the clipboard as «class PNGf»
    set fp to open for access POSIX file "{}" with write permission
    write imgData to fp
    close access fp
on error
    error "no png"
end try"#,
        png_path.to_string_lossy()
    );

    let png_result = Command::new("osascript")
        .arg("-e")
        .arg(&png_script)
        .output();

    if let Ok(ref o) = png_result {
        if o.status.success() {
            if let Ok(bytes) = std::fs::read(&png_path) {
                if !bytes.is_empty() {
                    return Ok(bytes);
                }
            }
        }
    }

    // --- Attempt 2: read clipboard as TIFF, convert via sips ---
    let tiff_path = std::env::temp_dir().join(format!("avis_clipboard_{pid}.tiff"));
    let _tiff_guard = TempFileGuard {
        path: tiff_path.clone(),
    };
    let converted_path = std::env::temp_dir().join(format!("avis_clipboard_{pid}_conv.png"));
    let _conv_guard = TempFileGuard {
        path: converted_path.clone(),
    };

    let tiff_script = format!(
        r#"try
    set imgData to the clipboard as «class TIFF»
    set fp to open for access POSIX file "{}" with write permission
    write imgData to fp
    close access fp
on error
    error "no tiff"
end try"#,
        tiff_path.to_string_lossy()
    );

    let tiff_result = Command::new("osascript")
        .arg("-e")
        .arg(&tiff_script)
        .output()
        .map_err(|e| VisionError::Capture(format!("Failed to run osascript: {e}")))?;

    if !tiff_result.status.success() {
        let stderr = String::from_utf8_lossy(&tiff_result.stderr);
        return Err(VisionError::Capture(format!(
            "No image found in clipboard (tried PNG and TIFF): {stderr}"
        )));
    }

    // Convert TIFF → PNG using sips (ships with macOS)
    let sips = Command::new("sips")
        .args([
            "-s",
            "format",
            "png",
            &tiff_path.to_string_lossy(),
            "--out",
            &converted_path.to_string_lossy(),
        ])
        .output()
        .map_err(|e| VisionError::Capture(format!("Failed to run sips: {e}")))?;

    if !sips.status.success() {
        let stderr = String::from_utf8_lossy(&sips.stderr);
        return Err(VisionError::Capture(format!(
            "Failed to convert TIFF clipboard image to PNG: {stderr}"
        )));
    }

    std::fs::read(&converted_path)
        .map_err(|e| VisionError::Capture(format!("Failed to read converted clipboard image: {e}")))
}

#[cfg(target_os = "linux")]
fn platform_clipboard_bytes() -> VisionResult<Vec<u8>> {
    // Try xclip first, then wl-paste (Wayland)
    let xclip = Command::new("xclip")
        .args(["-selection", "clipboard", "-t", "image/png", "-o"])
        .output();

    if let Ok(o) = xclip {
        if o.status.success() && !o.stdout.is_empty() {
            return Ok(o.stdout);
        }
    }

    let wl = Command::new("wl-paste")
        .args(["--type", "image/png"])
        .output();

    if let Ok(o) = wl {
        if o.status.success() && !o.stdout.is_empty() {
            return Ok(o.stdout);
        }
    }

    Err(VisionError::Capture(
        "No image in clipboard. Requires xclip or wl-paste.".to_string(),
    ))
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn platform_clipboard_bytes() -> VisionResult<Vec<u8>> {
    Err(VisionError::Capture(
        "Clipboard capture is not supported on this platform.".to_string(),
    ))
}

/// Capture a screenshot, optionally of a specific screen region.
///
/// On macOS, uses `screencapture -x`. On Linux, tries `gnome-screenshot`,
/// then falls back to `scrot` or `maim`. Windows is not currently supported.
pub fn capture_screenshot(region: Option<Rect>) -> VisionResult<(DynamicImage, CaptureSource)> {
    let temp_path =
        std::env::temp_dir().join(format!("avis_screenshot_{}.png", std::process::id()));
    let _guard = TempFileGuard {
        path: temp_path.clone(),
    };

    platform_screenshot(&temp_path, region)?;

    let img = image::open(&temp_path)
        .map_err(|e| VisionError::Capture(format!("Failed to read screenshot file: {e}")))?;

    Ok((img, CaptureSource::Screenshot { region }))
}

/// Capture an image from the system clipboard.
///
/// On macOS, uses `osascript` to extract PNG data. On Linux, uses `xclip`
/// or `wl-paste`. Windows is not currently supported.
pub fn capture_clipboard() -> VisionResult<(DynamicImage, CaptureSource)> {
    let image_bytes = platform_clipboard_bytes()?;

    if image_bytes.is_empty() {
        return Err(VisionError::Capture(
            "No image data found in clipboard.".to_string(),
        ));
    }

    let img = image::load_from_memory(&image_bytes)
        .map_err(|e| VisionError::Capture(format!("Failed to decode clipboard image: {e}")))?;

    Ok((img, CaptureSource::Clipboard))
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

    #[test]
    fn test_capture_screenshot_returns_sensible_result() {
        // On CI or headless environments, this will fail with a Capture error.
        // On a developer machine with display access, it may succeed.
        // We just verify it doesn't panic and returns the right error variant.
        let result = capture_screenshot(None);
        match result {
            Ok((img, CaptureSource::Screenshot { region: None })) => {
                let (w, h) = img.dimensions();
                assert!(w > 0 && h > 0);
            }
            Err(VisionError::Capture(_)) => {} // Expected on CI
            other => panic!("Unexpected result: {other:?}"),
        }
    }

    #[test]
    fn test_capture_clipboard_returns_sensible_result() {
        // On CI, clipboard is typically empty or inaccessible.
        let result = capture_clipboard();
        match result {
            Ok((img, CaptureSource::Clipboard)) => {
                let (w, h) = img.dimensions();
                assert!(w > 0 && h > 0);
            }
            Err(VisionError::Capture(_)) => {} // Expected on CI
            other => panic!("Unexpected result: {other:?}"),
        }
    }

    #[test]
    fn test_capture_screenshot_with_zero_region() {
        // Zero-size region — should not panic regardless of platform
        let region = Some(Rect {
            x: 0,
            y: 0,
            w: 0,
            h: 0,
        });
        let result = capture_screenshot(region);
        match result {
            Ok(_) | Err(VisionError::Capture(_)) => {}
            other => panic!("Unexpected result: {other:?}"),
        }
    }
}
