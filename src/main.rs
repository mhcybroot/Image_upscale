use anyhow::{Context, Result};
use clap::Parser;
use colored::*;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use image_upscale::cli::{Cli, ResampleFilter, ScaleMode};
use image_upscale::config::UpscaleConfig;
use image_upscale::engine::{ImageUpscaler, UpscaleOptions};
use image_upscale::license;

const SUPPORTED_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp", "bmp", "PNG", "JPG", "JPEG", "WEBP", "BMP"];

fn is_supported_image(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| SUPPORTED_EXTENSIONS.contains(&ext))
        .unwrap_or(false)
}

fn collect_images(input_path: &Path) -> Result<Vec<PathBuf>> {
    if input_path.is_file() {
        if is_supported_image(input_path) {
            Ok(vec![input_path.to_path_buf()])
        } else {
            anyhow::bail!(
                "Unsupported image format for file: {}",
                input_path.display()
            );
        }
    } else if input_path.is_dir() {
        let mut images = Vec::new();
        for entry in fs::read_dir(input_path)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() && is_supported_image(&path) {
                images.push(path);
            }
        }
        images.sort();
        Ok(images)
    } else {
        anyhow::bail!("Input path does not exist: {}", input_path.display());
    }
}

fn save_image(img: &image::RgbaImage, out_path: &Path, quality: u8) -> Result<()> {
    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let ext = out_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("png")
        .to_lowercase();

    match ext.as_str() {
        "jpg" | "jpeg" => {
            let rgb_img = image::DynamicImage::ImageRgba8(img.clone()).to_rgb8();
            let mut file = fs::File::create(out_path)?;
            let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut file, quality);
            encoder.encode_image(&rgb_img)?;
        }
        "webp" => {
            let dyn_img = image::DynamicImage::ImageRgba8(img.clone());
            dyn_img.save_with_format(out_path, image::ImageFormat::WebP)?;
        }
        "bmp" => {
            img.save_with_format(out_path, image::ImageFormat::Bmp)?;
        }
        _ => {
            img.save_with_format(out_path, image::ImageFormat::Png)?;
        }
    }
    Ok(())
}

fn main() -> Result<()> {
    let args = Cli::parse();

    // Handle Activation from CLI
    if let Some(ref key) = args.activate {
        match license::activate_product(key) {
            Ok(_) => {
                println!("{}", "==================================================".bright_blue());
                println!("  {}", "✅ PRODUCT SUCCESSFULLY ACTIVATED!".bold().bright_green());
                println!("  License has been registered on this machine.");
                println!("  Developed by: Mahmudul Hasan (Built with Rust 🦀)");
                println!("{}", "==================================================".bright_blue());
                return Ok(());
            }
            Err(e) => {
                println!("{}", "==================================================".bright_red());
                println!("  {}", "❌ ACTIVATION FAILED:".bold().bright_red());
                println!("  {}", e);
                println!("{}", "==================================================".bright_red());
                std::process::exit(1);
            }
        }
    }

    // Launch GUI if requested or when run with no arguments
    let args_vec: Vec<String> = std::env::args().collect();
    if args.gui || args_vec.len() == 1 {
        let native_options = eframe::NativeOptions {
            viewport: eframe::egui::ViewportBuilder::default()
                .with_inner_size([1120.0, 720.0])
                .with_min_inner_size([800.0, 500.0])
                .with_title("⚡ Fast SIMD Image Upscaler Pro (Mahmudul Hasan)"),
            ..Default::default()
        };

        let _ = eframe::run_native(
            "Image Upscaler Pro",
            native_options,
            Box::new(|cc| Ok(Box::new(image_upscale::gui::UpscalerGuiApp::new(cc)))),
        );
        return Ok(());
    }

    // Enforce license activation check for CLI processing
    if !license::is_product_activated() {
        println!("{}", "==================================================".bright_red());
        println!("  {}", "🔒 ACTIVATION REQUIRED:".bold().bright_yellow());
        println!("  This application is locked and requires product activation.");
        println!("  Run with: {} --activate <MASTER_PASSWORD>", "image_upscale".bold().cyan());
        println!("  Or launch the GUI to activate visually: {}", "image_upscale --gui".bold().cyan());
        println!("  Developer: Mahmudul Hasan");
        println!("{}", "==================================================".bright_red());
        std::process::exit(1);
    }

    let input_path = match args.input {
        Some(ref p) => p.clone(),
        None => {
            println!("{} Please provide input with -i/--input or launch GUI with --gui", "[ERROR]".red().bold());
            return Ok(());
        }
    };

    let output_path = match args.output {
        Some(ref p) => p.clone(),
        None => {
            println!("{} Please provide output with -o/--output", "[ERROR]".red().bold());
            return Ok(());
        }
    };

    // Check if configuration file was provided
    let (target_w, target_h, mode, filter, sharpen, quality) = if let Some(ref config_path) = args.config {
        let cfg = UpscaleConfig::load_from_file(config_path)?;
        println!(
            "{} Loaded configuration from {}",
            "[CONFIG]".cyan().bold(),
            config_path.display()
        );
        let mode = match cfg.mode.to_lowercase().as_str() {
            "fit" => ScaleMode::Fit,
            "fill" => ScaleMode::Fill,
            _ => ScaleMode::Stretch,
        };
        let filter = match cfg.filter.to_lowercase().as_str() {
            "catmullrom" | "catmull-rom" => ResampleFilter::CatmullRom,
            "mitchell" => ResampleFilter::Mitchell,
            "bilinear" => ResampleFilter::Bilinear,
            "nearest" => ResampleFilter::Nearest,
            _ => ResampleFilter::Lanczos3,
        };
        (cfg.target_width, cfg.target_height, mode, filter, cfg.sharpen, cfg.quality)
    } else {
        (args.width, args.height, args.mode, args.filter, args.sharpen, args.quality)
    };

    if args.threads > 0 {
        rayon::ThreadPoolBuilder::new()
            .num_threads(args.threads)
            .build_global()
            .ok();
    }

    let options = UpscaleOptions {
        target_width: target_w,
        target_height: target_h,
        mode,
        filter,
        sharpen_strength: sharpen,
    };

    let images = collect_images(&input_path)?;
    if images.is_empty() {
        println!("{} No compatible images found in {}", "[WARN]".yellow().bold(), input_path.display());
        return Ok(());
    }

    println!("{}", "==================================================".bright_blue());
    println!("{}", "  ⚡ FAST SIMD IMAGE UPSCALER & QUALITY ENHANCER  ".bold().bright_green());
    println!("{}", "      Developed by: Mahmudul Hasan (Built with Rust 🦀)".cyan());
    println!("{}", "==================================================".bright_blue());
    println!("  • Target Resolution: {}x{}", target_w.to_string().bold().yellow(), target_h.to_string().bold().yellow());
    println!("  • Scaling Mode     : {:?}", mode);
    println!("  • Filter Algorithm : {:?}", filter);
    println!("  • Sharpen Strength : {:.2}", sharpen);
    println!("  • Output Quality   : {}%", quality);
    println!("  • Images to process: {}", images.len().to_string().bold().cyan());
    println!("{}", "--------------------------------------------------".bright_blue());

    let is_dir_output = output_path.extension().is_none() || images.len() > 1;
    if is_dir_output {
        fs::create_dir_all(&output_path)?;
    }

    let progress_bar = ProgressBar::new(images.len() as u64);
    progress_bar.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta}) {msg}")
            .unwrap()
            .progress_chars("#>-"),
    );

    let start_time = Instant::now();

    // Process files in parallel
    let errors: Vec<_> = images
        .par_iter()
        .filter_map(|img_path| {
            let out_file_path = if is_dir_output {
                let file_name = img_path.file_name().unwrap();
                output_path.join(file_name)
            } else {
                output_path.clone()
            };

            let mut upscaler = ImageUpscaler::new();

            let process_result = (|| -> Result<()> {
                let dyn_img = image::open(img_path)
                    .with_context(|| format!("Failed to open image: {}", img_path.display()))?;
                
                let processed = upscaler.process_image(&dyn_img, &options)?;
                save_image(&processed, &out_file_path, quality)?;
                Ok(())
            })();

            progress_bar.inc(1);

            if let Err(e) = process_result {
                Some((img_path.clone(), e))
            } else {
                None
            }
        })
        .collect();

    progress_bar.finish_with_message("Done!");

    let duration = start_time.elapsed();
    let total_secs = duration.as_secs_f64();
    let count = images.len();
    let fps = count as f64 / total_secs.max(0.0001);
    let avg_ms = (total_secs * 1000.0) / count.max(1) as f64;

    println!("{}", "==================================================".bright_blue());
    if errors.is_empty() {
        println!("{} All {} images processed successfully!", "✔ SUCCESS:".green().bold(), count);
    } else {
        println!("{} {}/{} failed.", "✖ COMPLETED WITH ERRORS:".red().bold(), errors.len(), count);
        for (p, err) in errors.iter().take(5) {
            println!("  - {}: {}", p.display(), err);
        }
    }

    println!("  ⏱ Total Time : {:.3} s", total_secs);
    println!("  ⚡ Throughput : {:.1} FPS (avg {:.2} ms/image)", fps, avg_ms);
    println!("{}", "==================================================".bright_blue());

    Ok(())
}
