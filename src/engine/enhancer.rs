use image::{Rgba, RgbaImage};
use rayon::prelude::*;

/// Apply Contrast-Adaptive Sharpening (CAS) to enhance perceived picture quality.
/// CAS sharpens soft edges and textures while avoiding overshooting and ringing artifacts.
///
/// `strength`: 0.0 (no sharpening) to 1.0 (maximum sharpening). Recommended default: 0.3 - 0.4.
pub fn apply_adaptive_sharpen(img: &RgbaImage, strength: f32) -> RgbaImage {
    if strength <= 0.001 {
        return img.clone();
    }

    let width = img.width();
    let height = img.height();

    if width < 3 || height < 3 {
        return img.clone();
    }

    // Clamp strength between 0.0 and 1.0
    let sharpness = strength.clamp(0.0, 1.0);
    // Weight calculation inspired by AMD FidelityFX CAS
    let w = -0.125 * sharpness;

    let src_raw = img.as_raw();
    let mut dst_img = RgbaImage::new(width, height);
    let dst_raw = dst_img.as_mut();

    let row_stride = (width * 4) as usize;

    // Process rows in parallel using Rayon
    dst_raw
        .par_chunks_exact_mut(row_stride)
        .enumerate()
        .for_each(|(y, row_out)| {
            let y = y as u32;

            if y == 0 || y == height - 1 {
                // Copy boundary rows as-is
                let start_idx = (y * width * 4) as usize;
                let end_idx = start_idx + row_stride;
                row_out.copy_from_slice(&src_raw[start_idx..end_idx]);
                return;
            }

            for x in 0..width {
                let out_idx = (x * 4) as usize;

                if x == 0 || x == width - 1 {
                    // Copy boundary pixel as-is
                    let src_idx = (y * width + x) as usize * 4;
                    row_out[out_idx..out_idx + 4].copy_from_slice(&src_raw[src_idx..src_idx + 4]);
                    continue;
                }

                let center_idx = (y * width + x) as usize * 4;
                let top_idx = ((y - 1) * width + x) as usize * 4;
                let bottom_idx = ((y + 1) * width + x) as usize * 4;
                let left_idx = (y * width + (x - 1)) as usize * 4;
                let right_idx = (y * width + (x + 1)) as usize * 4;

                // Process R, G, B channels independently; keep Alpha intact
                for c in 0..3 {
                    let center = src_raw[center_idx + c] as f32;
                    let top = src_raw[top_idx + c] as f32;
                    let bottom = src_raw[bottom_idx + c] as f32;
                    let left = src_raw[left_idx + c] as f32;
                    let right = src_raw[right_idx + c] as f32;

                    // Local min / max to prevent haloing
                    let min_val = center.min(top).min(bottom).min(left).min(right);
                    let max_val = center.max(top).max(bottom).max(left).max(right);

                    // Adaptive weight factor based on contrast range
                    let amp = ((min_val / (max_val + 1.0)).min((255.0 - max_val) / (255.0 - min_val + 1.0)))
                        .clamp(0.0, 1.0);
                    let peak_w = w * amp;

                    let total_weight = 1.0 + 4.0 * peak_w;
                    let sum = center + peak_w * (top + bottom + left + right);
                    let enhanced = sum / total_weight;

                    row_out[out_idx + c] = enhanced.clamp(0.0, 255.0).round() as u8;
                }

                // Copy original alpha
                row_out[out_idx + 3] = src_raw[center_idx + 3];
            }
        });

    dst_img
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sharpen_dimensions() {
        let img = RgbaImage::from_pixel(64, 64, Rgba([128, 128, 128, 255]));
        let sharpened = apply_adaptive_sharpen(&img, 0.5);
        assert_eq!(sharpened.width(), 64);
        assert_eq!(sharpened.height(), 64);
    }
}
