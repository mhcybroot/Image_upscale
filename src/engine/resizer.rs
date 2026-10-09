use crate::cli::{ResampleFilter, ScaleMode};
use crate::engine::enhancer::apply_adaptive_sharpen;
use anyhow::{Context, Result};
use fast_image_resize::images::Image;
use fast_image_resize::{FilterType, PixelType, ResizeAlg, ResizeOptions, Resizer};
use image::{DynamicImage, GenericImageView, ImageBuffer, Rgba, RgbaImage};

/// Process options for image upscaling
#[derive(Debug, Clone)]
pub struct UpscaleOptions {
    pub target_width: u32,
    pub target_height: u32,
    pub mode: ScaleMode,
    pub filter: ResampleFilter,
    pub sharpen_strength: f32,
}

impl Default for UpscaleOptions {
    fn default() -> Self {
        Self {
            target_width: 720,
            target_height: 480,
            mode: ScaleMode::Stretch,
            filter: ResampleFilter::Lanczos3,
            sharpen_strength: 0.35,
        }
    }
}

pub struct ImageUpscaler {
    resizer: Resizer,
}

impl Default for ImageUpscaler {
    fn default() -> Self {
        Self::new()
    }
}

impl ImageUpscaler {
    pub fn new() -> Self {
        Self {
            resizer: Resizer::new(),
        }
    }

    /// Upscale and enhance a dynamic image according to given options
    pub fn process_image(
        &mut self,
        img: &DynamicImage,
        options: &UpscaleOptions,
    ) -> Result<RgbaImage> {
        let (src_w, src_h) = img.dimensions();
        let target_w = options.target_width;
        let target_h = options.target_height;

        // If dimensions already match and no sharpening needed
        if src_w == target_w && src_h == target_h && options.sharpen_strength <= 0.001 {
            return Ok(img.to_rgba8());
        }

        let rgba_img = img.to_rgba8();

        let intermediate = match options.mode {
            ScaleMode::Stretch => {
                self.resize_rgba(&rgba_img, target_w, target_h, options.filter)?
            }
            ScaleMode::Fit => {
                // Calculate aspect ratio preserving fit
                let scale = (target_w as f64 / src_w as f64).min(target_h as f64 / src_h as f64);
                let scaled_w = ((src_w as f64 * scale).round() as u32).max(1).min(target_w);
                let scaled_h = ((src_h as f64 * scale).round() as u32).max(1).min(target_h);

                let resized_content =
                    self.resize_rgba(&rgba_img, scaled_w, scaled_h, options.filter)?;

                // Create background canvas of target_w x target_h
                let mut canvas = ImageBuffer::from_pixel(target_w, target_h, Rgba([0, 0, 0, 255]));
                let offset_x = (target_w - scaled_w) / 2;
                let offset_y = (target_h - scaled_h) / 2;

                image::imageops::overlay(&mut canvas, &resized_content, offset_x as i64, offset_y as i64);
                canvas
            }
            ScaleMode::Fill => {
                // Calculate aspect ratio preserving fill and crop
                let scale = (target_w as f64 / src_w as f64).max(target_h as f64 / src_h as f64);
                let scaled_w = ((src_w as f64 * scale).round() as u32).max(target_w);
                let scaled_h = ((src_h as f64 * scale).round() as u32).max(target_h);

                let resized_content =
                    self.resize_rgba(&rgba_img, scaled_w, scaled_h, options.filter)?;

                // Crop center
                let crop_x = (scaled_w - target_w) / 2;
                let crop_y = (scaled_h - target_h) / 2;

                let cropped = image::imageops::crop_imm(
                    &resized_content,
                    crop_x,
                    crop_y,
                    target_w,
                    target_h,
                )
                .to_image();
                cropped
            }
        };

        // Apply picture quality enhancement / sharpening
        let enhanced = apply_adaptive_sharpen(&intermediate, options.sharpen_strength);
        Ok(enhanced)
    }

    /// Fast SIMD resizing of RGBA8 buffer
    fn resize_rgba(
        &mut self,
        src: &RgbaImage,
        dst_width: u32,
        dst_height: u32,
        filter: ResampleFilter,
    ) -> Result<RgbaImage> {
        let (src_w, src_h) = (src.width(), src.height());

        if src_w == 0 || src_h == 0 {
            anyhow::bail!("Source dimensions must be non-zero");
        }
        if dst_width == 0 || dst_height == 0 {
            anyhow::bail!("Target dimensions must be non-zero");
        }

        let src_image = Image::from_vec_u8(
            src_w,
            src_h,
            src.clone().into_raw(),
            PixelType::U8x4,
        )
        .map_err(|e| anyhow::anyhow!("Failed to create source image buffer: {:?}", e))?;

        let mut dst_image = Image::new(dst_width, dst_height, PixelType::U8x4);

        let resize_alg = match filter {
            ResampleFilter::Lanczos3 => ResizeAlg::Convolution(FilterType::Lanczos3),
            ResampleFilter::CatmullRom => ResizeAlg::Convolution(FilterType::CatmullRom),
            ResampleFilter::Mitchell => ResizeAlg::Convolution(FilterType::Mitchell),
            ResampleFilter::Bilinear => ResizeAlg::Convolution(FilterType::Bilinear),
            ResampleFilter::Nearest => ResizeAlg::Nearest,
        };

        let resize_options = ResizeOptions::new().resize_alg(resize_alg);

        self.resizer
            .resize(&src_image, &mut dst_image, &resize_options)
            .map_err(|e| anyhow::anyhow!("Resize error: {:?}", e))?;

        let dst_raw = dst_image.into_vec();
        let result_img = ImageBuffer::from_raw(dst_width, dst_height, dst_raw)
            .context("Failed to reconstruct image buffer from resized raw data")?;

        Ok(result_img)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_upscale_640x480_to_720x480() {
        let img = DynamicImage::ImageRgba8(RgbaImage::new(640, 480));
        let mut upscaler = ImageUpscaler::new();
        let opts = UpscaleOptions {
            target_width: 720,
            target_height: 480,
            mode: ScaleMode::Stretch,
            filter: ResampleFilter::Lanczos3,
            sharpen_strength: 0.35,
        };

        let result = upscaler.process_image(&img, &opts).unwrap();
        assert_eq!(result.width(), 720);
        assert_eq!(result.height(), 480);
    }

    #[test]
    fn test_upscale_fit_mode() {
        let img = DynamicImage::ImageRgba8(RgbaImage::new(640, 480));
        let mut upscaler = ImageUpscaler::new();
        let opts = UpscaleOptions {
            target_width: 720,
            target_height: 480,
            mode: ScaleMode::Fit,
            filter: ResampleFilter::Lanczos3,
            sharpen_strength: 0.0,
        };

        let result = upscaler.process_image(&img, &opts).unwrap();
        assert_eq!(result.width(), 720);
        assert_eq!(result.height(), 480);
    }
}
