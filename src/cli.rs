use clap::{Parser, ValueEnum};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
pub enum ScaleMode {
    /// Directly resize to exact target dimensions (default)
    Stretch,
    /// Preserve aspect ratio with background padding/letterboxing
    Fit,
    /// Preserve aspect ratio, scale to fill target and crop excess
    Fill,
}

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
pub enum ResampleFilter {
    Lanczos3,
    CatmullRom,
    Mitchell,
    Bilinear,
    Nearest,
}

#[derive(Parser, Debug)]
#[command(
    name = "image_upscale",
    author = "Mahmudul Hasan",
    version = "0.1.0",
    about = "Blazing-fast SIMD image upscaler and quality enhancer by Mahmudul Hasan (Linux & Windows)",
    long_about = "A high-performance CLI utility for upscaling and enhancing image quality.\n\
                  Supports arbitrary input resolutions with a default output resolution of 720x480 (customizable).\n\
                  Developed with Rust by Mahmudul Hasan using hardware SIMD and multi-threading."
)]
pub struct Cli {
    /// Path to input image file or directory containing images
    #[arg(short, long, value_name = "PATH")]
    pub input: Option<PathBuf>,

    /// Path to output image file or output directory
    #[arg(short, long, value_name = "PATH")]
    pub output: Option<PathBuf>,

    /// Target image width in pixels (default: 720)
    #[arg(short = 'W', long = "width", default_value_t = 720)]
    pub width: u32,

    /// Target image height in pixels (default: 480)
    #[arg(short = 'H', long = "height", default_value_t = 480)]
    pub height: u32,

    /// Scaling mode when aspect ratios differ
    #[arg(short, long, value_enum, default_value_t = ScaleMode::Stretch)]
    pub mode: ScaleMode,

    /// Resampling algorithm filter
    #[arg(short, long, value_enum, default_value_t = ResampleFilter::Lanczos3)]
    pub filter: ResampleFilter,

    /// Adaptive sharpening strength (0.0 = disabled, 0.35 = balanced, 1.0 = sharpest)
    #[arg(short, long, default_value_t = 0.35)]
    pub sharpen: f32,

    /// Output JPEG/WebP quality (1-100)
    #[arg(short, long, default_value_t = 95)]
    pub quality: u8,

    /// Number of parallel worker threads (0 = auto / all CPU cores)
    #[arg(short = 't', long, default_value_t = 0)]
    pub threads: usize,

    /// Path to custom config file (e.g. config.toml)
    #[arg(long, value_name = "FILE")]
    pub config: Option<PathBuf>,

    /// Activate product with Master Password
    #[arg(long, value_name = "KEY")]
    pub activate: Option<String>,

    /// Launch Graphical User Interface (GUI)
    #[arg(short = 'g', long)]
    pub gui: bool,

    /// Run benchmark mode to measure throughput (FPS & ms per image)
    #[arg(long)]
    pub benchmark: bool,
}
