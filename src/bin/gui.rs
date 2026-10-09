#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;
use image_upscale::gui::UpscalerGuiApp;

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1120.0, 720.0])
            .with_min_inner_size([800.0, 500.0])
            .with_title("⚡ Fast SIMD Image Upscaler (720x480 & Custom)"),
        ..Default::default()
    };

    eframe::run_native(
        "Image Upscaler & Quality Enhancer",
        native_options,
        Box::new(|cc| Ok(Box::new(UpscalerGuiApp::new(cc)))),
    )
}
