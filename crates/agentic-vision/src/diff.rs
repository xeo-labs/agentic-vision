//! Change detection between visual captures.

use image::{DynamicImage, GenericImageView, GrayImage, Luma};

use crate::types::{Rect, VisualDiff, VisionResult};

/// Pixel difference threshold (0-255) for considering a pixel "changed".
const DIFF_THRESHOLD: u8 = 30;

/// Minimum region size (pixels) to report as a changed region.
const MIN_REGION_SIZE: u32 = 10;

/// Compute a visual diff between two images (provided as JPEG thumbnail bytes).
pub fn compute_diff(
    before_id: u64,
    after_id: u64,
    img_a: &DynamicImage,
    img_b: &DynamicImage,
) -> VisionResult<VisualDiff> {
    let (w_a, h_a) = img_a.dimensions();
    let (w_b, h_b) = img_b.dimensions();

    // Resize to common dimensions for comparison
    let target_w = w_a.min(w_b);
    let target_h = h_a.min(h_b);

    let gray_a = img_a
        .resize_exact(target_w, target_h, image::imageops::FilterType::Nearest)
        .to_luma8();
    let gray_b = img_b
        .resize_exact(target_w, target_h, image::imageops::FilterType::Nearest)
        .to_luma8();

    // Compute per-pixel absolute difference
    let mut diff_img = GrayImage::new(target_w, target_h);
    let mut changed_pixels = 0u64;
    let total_pixels = (target_w as u64) * (target_h as u64);

    for y in 0..target_h {
        for x in 0..target_w {
            let a = gray_a.get_pixel(x, y).0[0];
            let b = gray_b.get_pixel(x, y).0[0];
            let d = (a as i16 - b as i16).unsigned_abs() as u8;
            diff_img.put_pixel(x, y, Luma([d]));
            if d > DIFF_THRESHOLD {
                changed_pixels += 1;
            }
        }
    }

    let pixel_diff_ratio = if total_pixels > 0 {
        changed_pixels as f32 / total_pixels as f32
    } else {
        0.0
    };

    let similarity = 1.0 - pixel_diff_ratio;
    let changed_regions = find_changed_regions(&diff_img, DIFF_THRESHOLD);

    Ok(VisualDiff {
        before_id,
        after_id,
        similarity,
        changed_regions,
        pixel_diff_ratio,
    })
}

/// Find bounding boxes of changed regions using simple grid-based detection.
fn find_changed_regions(diff_img: &GrayImage, threshold: u8) -> Vec<Rect> {
    let (w, h) = diff_img.dimensions();
    if w == 0 || h == 0 {
        return Vec::new();
    }

    // Divide image into a grid and find cells with significant changes
    let cell_w = (w / 8).max(1);
    let cell_h = (h / 8).max(1);
    let mut regions = Vec::new();

    for gy in 0..(h / cell_h).max(1) {
        for gx in 0..(w / cell_w).max(1) {
            let x0 = gx * cell_w;
            let y0 = gy * cell_h;
            let x1 = ((gx + 1) * cell_w).min(w);
            let y1 = ((gy + 1) * cell_h).min(h);

            let mut changed = 0u32;
            let total = (x1 - x0) * (y1 - y0);

            for y in y0..y1 {
                for x in x0..x1 {
                    if diff_img.get_pixel(x, y).0[0] > threshold {
                        changed += 1;
                    }
                }
            }

            // If more than 10% of cell pixels changed, mark this region
            if total > 0 && changed > total / 10 && (x1 - x0) >= MIN_REGION_SIZE {
                regions.push(Rect {
                    x: x0,
                    y: y0,
                    w: x1 - x0,
                    h: y1 - y0,
                });
            }
        }
    }

    // Merge adjacent regions
    merge_adjacent_regions(&mut regions);
    regions
}

/// Merge adjacent or overlapping rectangles.
fn merge_adjacent_regions(regions: &mut Vec<Rect>) {
    if regions.len() < 2 {
        return;
    }

    let mut merged = true;
    while merged {
        merged = false;
        let mut i = 0;
        while i < regions.len() {
            let mut j = i + 1;
            while j < regions.len() {
                if rects_adjacent(&regions[i], &regions[j]) {
                    let a = regions[i];
                    let b = regions.remove(j);
                    regions[i] = merge_rects(&a, &b);
                    merged = true;
                } else {
                    j += 1;
                }
            }
            i += 1;
        }
    }
}

fn rects_adjacent(a: &Rect, b: &Rect) -> bool {
    let a_right = a.x + a.w;
    let a_bottom = a.y + a.h;
    let b_right = b.x + b.w;
    let b_bottom = b.y + b.h;

    // Check if they overlap or touch
    !(a_right < b.x || b_right < a.x || a_bottom < b.y || b_bottom < a.y)
}

fn merge_rects(a: &Rect, b: &Rect) -> Rect {
    let x = a.x.min(b.x);
    let y = a.y.min(b.y);
    let right = (a.x + a.w).max(b.x + b.w);
    let bottom = (a.y + a.h).max(b.y + b.h);
    Rect {
        x,
        y,
        w: right - x,
        h: bottom - y,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identical_images() {
        let img = DynamicImage::new_rgb8(100, 100);
        let diff = compute_diff(1, 2, &img, &img).unwrap();
        assert!((diff.similarity - 1.0).abs() < 0.01);
        assert!(diff.pixel_diff_ratio < 0.01);
    }

    #[test]
    fn test_different_images() {
        let mut img_a = DynamicImage::new_rgb8(100, 100);
        let img_b = DynamicImage::new_rgba8(100, 100);
        // Fill img_a with white
        if let Some(rgb) = img_a.as_mut_rgb8() {
            for pixel in rgb.pixels_mut() {
                *pixel = image::Rgb([255, 255, 255]);
            }
        }
        let diff = compute_diff(1, 2, &img_a, &img_b).unwrap();
        assert!(diff.similarity < 1.0);
    }
}
