use eframe::egui;
use image::{DynamicImage, GenericImageView, RgbaImage};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::Instant;

use crate::cli::{ResampleFilter, ScaleMode};
use crate::engine::{ImageUpscaler, UpscaleOptions};

const SUPPORTED_EXTS: &[&str] = &["png", "jpg", "jpeg", "webp", "bmp", "PNG", "JPG", "JPEG", "WEBP", "BMP"];

fn is_supported(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| SUPPORTED_EXTS.contains(&ext))
        .unwrap_or(false)
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum AppTab {
    Single,
    Batch,
}

#[derive(Clone, PartialEq, Eq)]
pub enum BatchStatus {
    Pending,
    Processing,
    Completed(PathBuf),
    Failed(String),
}

pub struct BatchItem {
    pub path: PathBuf,
    pub name: String,
    pub dimensions: Option<(u32, u32)>,
    pub thumbnail_texture: Option<egui::TextureHandle>,
    pub status: BatchStatus,
}

enum GuiEvent {
    // Single mode
    SingleImageLoaded {
        path: PathBuf,
        image: DynamicImage,
    },
    SingleLoadFailed(String),
    SingleUpscaleCompleted {
        image: RgbaImage,
        duration_ms: f64,
        target_w: u32,
        target_h: u32,
    },
    SingleUpscaleFailed(String),
    SingleSaveCompleted(PathBuf),
    SingleSaveFailed(String),

    // Batch mode
    BatchItemsInitialized {
        items: Vec<BatchItem>,
        source_desc: String,
    },
    ThumbnailLoaded {
        index: usize,
        rgba: RgbaImage,
        dims: (u32, u32),
    },
    BatchItemProgress {
        index: usize,
        status: BatchStatus,
        completed_count: usize,
    },
    BatchFinished {
        total: usize,
        success_count: usize,
        error_count: usize,
        duration_secs: f64,
    },
}

pub struct UpscalerGuiApp {
    current_tab: AppTab,

    // Single mode state
    single_input_path: Option<PathBuf>,
    single_input_image: Option<DynamicImage>,
    single_input_texture: Option<egui::TextureHandle>,
    single_processed_image: Option<RgbaImage>,
    single_processed_texture: Option<egui::TextureHandle>,
    single_last_ms: Option<f64>,

    // Batch mode state
    batch_items: Vec<BatchItem>,
    batch_source_desc: String,
    batch_output_dir: Option<PathBuf>,
    batch_total: usize,
    batch_completed: usize,
    batch_is_running: Arc<AtomicBool>,
    batch_summary: Option<String>,
    batch_fps: Option<f64>,

    // Settings
    target_width: u32,
    target_height: u32,
    mode: ScaleMode,
    filter: ResampleFilter,
    sharpen: f32,
    quality: u8,
    lock_aspect_ratio: bool,
    saved_aspect_ratio: f64,

    // Status & Modals
    status_message: String,
    is_processing_single: bool,
    show_about_dialog: bool,

    // Activation Lock State
    is_activated: bool,
    activation_input: String,
    activation_error: Option<String>,

    // Channels
    tx: Sender<GuiEvent>,
    rx: Receiver<GuiEvent>,
}

impl Default for UpscalerGuiApp {
    fn default() -> Self {
        let (tx, rx) = channel();
        let activated = crate::license::is_product_activated();
        Self {
            current_tab: AppTab::Single,
            single_input_path: None,
            single_input_image: None,
            single_input_texture: None,
            single_processed_image: None,
            single_processed_texture: None,
            single_last_ms: None,

            batch_items: Vec::new(),
            batch_source_desc: "No files or folder selected.".to_string(),
            batch_output_dir: None,
            batch_total: 0,
            batch_completed: 0,
            batch_is_running: Arc::new(AtomicBool::new(false)),
            batch_summary: None,
            batch_fps: None,

            target_width: 720,
            target_height: 480,
            mode: ScaleMode::Stretch,
            filter: ResampleFilter::Lanczos3,
            sharpen: 0.35,
            quality: 95,
            lock_aspect_ratio: false,
            saved_aspect_ratio: 720.0 / 480.0,

            status_message: "Ready. Select an image or folder to begin.".to_string(),
            is_processing_single: false,
            show_about_dialog: false,
            is_activated: activated,
            activation_input: String::new(),
            activation_error: None,
            tx,
            rx,
        }
    }
}

impl UpscalerGuiApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        egui_extras::install_image_loaders(&cc.egui_ctx);
        
        // Configure dark theme style
        let mut visuals = egui::Visuals::dark();
        visuals.window_rounding = egui::Rounding::same(10.0);
        visuals.window_fill = egui::Color32::from_rgb(18, 22, 30);
        visuals.panel_fill = egui::Color32::from_rgb(15, 18, 25);
        visuals.widgets.noninteractive.bg_fill = egui::Color32::from_rgb(22, 28, 38);
        visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(30, 38, 52);
        visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(45, 56, 75);
        visuals.widgets.active.bg_fill = egui::Color32::from_rgb(0, 180, 120);
        visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);
        visuals.selection.bg_fill = egui::Color32::from_rgb(0, 200, 130);
        cc.egui_ctx.set_visuals(visuals);

        Self::default()
    }

    // --- Single Image Actions ---
    fn pick_single_image(&self) {
        let tx = self.tx.clone();
        thread::spawn(move || {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Images (*.png, *.jpg, *.webp, *.bmp)", &["png", "jpg", "jpeg", "webp", "bmp"])
                .pick_file()
            {
                match image::open(&path) {
                    Ok(dyn_img) => {
                        let _ = tx.send(GuiEvent::SingleImageLoaded { path, image: dyn_img });
                    }
                    Err(e) => {
                        let _ = tx.send(GuiEvent::SingleLoadFailed(format!("Failed to open image: {}", e)));
                    }
                }
            }
        });
    }

    fn upscale_single(&mut self) {
        let dyn_img = match self.single_input_image.as_ref() {
            Some(img) => img.clone(),
            None => {
                self.status_message = "Please select an image first.".to_string();
                return;
            }
        };

        self.is_processing_single = true;
        self.status_message = "⏳ Processing image with SIMD acceleration...".to_string();

        let tx = self.tx.clone();
        let target_w = self.target_width;
        let target_h = self.target_height;
        let options = UpscaleOptions {
            target_width: target_w,
            target_height: target_h,
            mode: self.mode,
            filter: self.filter,
            sharpen_strength: self.sharpen,
        };

        thread::spawn(move || {
            let start = Instant::now();
            let mut upscaler = ImageUpscaler::new();
            match upscaler.process_image(&dyn_img, &options) {
                Ok(processed) => {
                    let duration_ms = start.elapsed().as_secs_f64() * 1000.0;
                    let _ = tx.send(GuiEvent::SingleUpscaleCompleted {
                        image: processed,
                        duration_ms,
                        target_w,
                        target_h,
                    });
                }
                Err(e) => {
                    let _ = tx.send(GuiEvent::SingleUpscaleFailed(format!("Upscale failed: {}", e)));
                }
            }
        });
    }

    fn save_single_image(&self) {
        let processed = match self.single_processed_image.as_ref() {
            Some(img) => img.clone(),
            None => return,
        };

        let tx = self.tx.clone();
        let quality = self.quality;

        thread::spawn(move || {
            let file_dialog = rfd::FileDialog::new()
                .set_file_name("upscaled_720x480.png")
                .add_filter("PNG Image (*.png)", &["png"])
                .add_filter("JPEG Image (*.jpg, *.jpeg)", &["jpg", "jpeg"])
                .add_filter("WebP Image (*.webp)", &["webp"]);

            if let Some(path) = file_dialog.save_file() {
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("png").to_lowercase();
                let res = match ext.as_str() {
                    "jpg" | "jpeg" => {
                        let rgb = image::DynamicImage::ImageRgba8(processed).to_rgb8();
                        match fs::File::create(&path) {
                            Ok(mut f) => {
                                let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut f, quality);
                                encoder.encode_image(&rgb).map_err(|e| anyhow::anyhow!(e))
                            }
                            Err(e) => Err(anyhow::anyhow!(e)),
                        }
                    }
                    "webp" => {
                        let dyn_img = image::DynamicImage::ImageRgba8(processed);
                        dyn_img.save_with_format(&path, image::ImageFormat::WebP).map_err(|e| anyhow::anyhow!(e))
                    }
                    _ => processed.save_with_format(&path, image::ImageFormat::Png).map_err(|e| anyhow::anyhow!(e)),
                };

                match res {
                    Ok(_) => {
                        let _ = tx.send(GuiEvent::SingleSaveCompleted(path));
                    }
                    Err(e) => {
                        let _ = tx.send(GuiEvent::SingleSaveFailed(format!("Save error: {}", e)));
                    }
                }
            }
        });
    }

    // --- Batch Actions ---
    fn pick_batch_folder(&self) {
        let tx = self.tx.clone();
        thread::spawn(move || {
            if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                let mut paths = Vec::new();
                if let Ok(entries) = fs::read_dir(&dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_file() && is_supported(&path) {
                            paths.push(path);
                        }
                    }
                }
                paths.sort();
                let desc = format!("Folder: {} ({} images)", dir.file_name().unwrap_or_default().to_string_lossy(), paths.len());
                Self::init_batch_items(paths, desc, tx);
            }
        });
    }

    fn pick_multiple_files(&self) {
        let tx = self.tx.clone();
        thread::spawn(move || {
            if let Some(paths) = rfd::FileDialog::new()
                .add_filter("Images (*.png, *.jpg, *.webp, *.bmp)", &["png", "jpg", "jpeg", "webp", "bmp"])
                .pick_files()
            {
                let files: Vec<PathBuf> = paths.into_iter().filter(|p| is_supported(p)).collect();
                let desc = format!("{} files selected", files.len());
                Self::init_batch_items(files, desc, tx);
            }
        });
    }

    fn init_batch_items(paths: Vec<PathBuf>, desc: String, tx: Sender<GuiEvent>) {
        let mut items = Vec::new();
        for path in &paths {
            let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
            items.push(BatchItem {
                path: path.clone(),
                name,
                dimensions: None,
                thumbnail_texture: None,
                status: BatchStatus::Pending,
            });
        }

        let _ = tx.send(GuiEvent::BatchItemsInitialized {
            items,
            source_desc: desc,
        });

        // Generate thumbnails concurrently in background
        for (idx, path) in paths.into_iter().enumerate() {
            let tx_thumb = tx.clone();
            thread::spawn(move || {
                if let Ok(dyn_img) = image::open(&path) {
                    let dims = (dyn_img.width(), dyn_img.height());
                    let thumb = dyn_img.thumbnail(200, 150).to_rgba8();
                    let _ = tx_thumb.send(GuiEvent::ThumbnailLoaded {
                        index: idx,
                        rgba: thumb,
                        dims,
                    });
                }
            });
        }
    }

    fn pick_batch_output_folder(&mut self) {
        if let Some(dir) = rfd::FileDialog::new().pick_folder() {
            self.batch_output_dir = Some(dir);
        }
    }

    fn open_output_folder(&self) {
        let out_dir = if let Some(dir) = &self.batch_output_dir {
            dir.clone()
        } else if !self.batch_items.is_empty() {
            let first_parent = self.batch_items[0].path.parent().unwrap_or_else(|| Path::new("."));
            first_parent.join("upscaled_outputs")
        } else {
            PathBuf::from("test_outputs")
        };

        let _ = open::that(&out_dir);
    }

    fn start_batch_processing(&mut self) {
        if self.batch_items.is_empty() {
            self.status_message = "No images in batch queue.".to_string();
            return;
        }

        let out_dir = match &self.batch_output_dir {
            Some(dir) => dir.clone(),
            None => {
                let first_parent = self.batch_items[0].path.parent().unwrap_or_else(|| Path::new("."));
                let default_out = first_parent.join("upscaled_outputs");
                let _ = fs::create_dir_all(&default_out);
                self.batch_output_dir = Some(default_out.clone());
                default_out
            }
        };

        self.batch_total = self.batch_items.len();
        self.batch_completed = 0;
        self.batch_summary = None;
        self.batch_is_running.store(true, Ordering::SeqCst);

        let is_running = self.batch_is_running.clone();
        let items_paths: Vec<(usize, PathBuf)> = self.batch_items.iter().enumerate().map(|(i, it)| (i, it.path.clone())).collect();
        let tx = self.tx.clone();
        let quality = self.quality;
        let options = UpscaleOptions {
            target_width: self.target_width,
            target_height: self.target_height,
            mode: self.mode,
            filter: self.filter,
            sharpen_strength: self.sharpen,
        };

        thread::spawn(move || {
            let start = Instant::now();
            let total = items_paths.len();
            let mut success_count = 0;
            let mut error_count = 0;

            for (idx, path) in items_paths {
                if !is_running.load(Ordering::SeqCst) {
                    break;
                }

                let _ = tx.send(GuiEvent::BatchItemProgress {
                    index: idx,
                    status: BatchStatus::Processing,
                    completed_count: idx,
                });

                let file_name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                let out_file_path = out_dir.join(&file_name);
                let mut upscaler = ImageUpscaler::new();
                let item_status;

                match image::open(&path) {
                    Ok(dyn_img) => match upscaler.process_image(&dyn_img, &options) {
                        Ok(processed) => {
                            let ext = out_file_path.extension().and_then(|e| e.to_str()).unwrap_or("png").to_lowercase();
                            let save_res = match ext.as_str() {
                                "jpg" | "jpeg" => {
                                    let rgb = image::DynamicImage::ImageRgba8(processed).to_rgb8();
                                    match fs::File::create(&out_file_path) {
                                        Ok(mut f) => {
                                            let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut f, quality);
                                            encoder.encode_image(&rgb).map_err(|e| anyhow::anyhow!(e))
                                        }
                                        Err(e) => Err(anyhow::anyhow!(e)),
                                    }
                                }
                                "webp" => {
                                    let dyn_img = image::DynamicImage::ImageRgba8(processed);
                                    dyn_img.save_with_format(&out_file_path, image::ImageFormat::WebP).map_err(|e| anyhow::anyhow!(e))
                                }
                                _ => processed.save_with_format(&out_file_path, image::ImageFormat::Png).map_err(|e| anyhow::anyhow!(e)),
                            };

                            if save_res.is_ok() {
                                success_count += 1;
                                item_status = BatchStatus::Completed(out_file_path);
                            } else {
                                error_count += 1;
                                item_status = BatchStatus::Failed("Save failed".to_string());
                            }
                        }
                        Err(e) => {
                            error_count += 1;
                            item_status = BatchStatus::Failed(format!("Upscale error: {}", e));
                        }
                    },
                    Err(e) => {
                        error_count += 1;
                        item_status = BatchStatus::Failed(format!("Open error: {}", e));
                    }
                }

                let _ = tx.send(GuiEvent::BatchItemProgress {
                    index: idx,
                    status: item_status,
                    completed_count: idx + 1,
                });
            }

            let duration_secs = start.elapsed().as_secs_f64();
            is_running.store(false, Ordering::SeqCst);
            let _ = tx.send(GuiEvent::BatchFinished {
                total,
                success_count,
                error_count,
                duration_secs,
            });
        });
    }

    fn check_background_events(&mut self, ctx: &egui::Context) {
        while let Ok(event) = self.rx.try_recv() {
            match event {
                // Single mode
                GuiEvent::SingleImageLoaded { path, image } => {
                    let rgba = image.to_rgba8();
                    let color_image = egui::ColorImage::from_rgba_unmultiplied(
                        [image.width() as usize, image.height() as usize],
                        rgba.as_raw(),
                    );
                    let texture = ctx.load_texture("input_preview", color_image, Default::default());

                    let w = image.width();
                    let h = image.height();
                    let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();

                    self.saved_aspect_ratio = w as f64 / h.max(1) as f64;
                    self.single_input_texture = Some(texture);
                    self.single_input_image = Some(image);
                    self.single_input_path = Some(path);
                    self.single_processed_image = None;
                    self.single_processed_texture = None;
                    self.single_last_ms = None;
                    self.status_message = format!("📷 Loaded: {} ({}x{} px)", name, w, h);
                }
                GuiEvent::SingleLoadFailed(err) => {
                    self.status_message = format!("❌ {}", err);
                }
                GuiEvent::SingleUpscaleCompleted {
                    image,
                    duration_ms,
                    target_w,
                    target_h,
                } => {
                    self.is_processing_single = false;
                    self.single_last_ms = Some(duration_ms);

                    let color_image = egui::ColorImage::from_rgba_unmultiplied(
                        [image.width() as usize, image.height() as usize],
                        image.as_raw(),
                    );
                    let texture = ctx.load_texture("output_preview", color_image, Default::default());

                    self.single_processed_texture = Some(texture);
                    self.single_processed_image = Some(image);
                    self.status_message = format!(
                        "✨ Upscaled successfully in {:.2} ms (Target: {}x{} px)!",
                        duration_ms, target_w, target_h
                    );
                }
                GuiEvent::SingleUpscaleFailed(err) => {
                    self.is_processing_single = false;
                    self.status_message = format!("❌ {}", err);
                }
                GuiEvent::SingleSaveCompleted(path) => {
                    self.status_message = format!("💾 Saved successfully to: {}", path.display());
                }
                GuiEvent::SingleSaveFailed(err) => {
                    self.status_message = format!("❌ {}", err);
                }

                // Batch mode
                GuiEvent::BatchItemsInitialized { items, source_desc } => {
                    self.batch_items = items;
                    self.batch_source_desc = source_desc;
                    self.batch_total = self.batch_items.len();
                    self.batch_completed = 0;
                    self.batch_summary = None;
                    self.batch_fps = None;
                    self.status_message = format!("📂 Loaded {} images into batch queue.", self.batch_items.len());
                }
                GuiEvent::ThumbnailLoaded { index, rgba, dims } => {
                    if let Some(item) = self.batch_items.get_mut(index) {
                        item.dimensions = Some(dims);
                        let color_image = egui::ColorImage::from_rgba_unmultiplied(
                            [rgba.width() as usize, rgba.height() as usize],
                            rgba.as_raw(),
                        );
                        let tex = ctx.load_texture(format!("thumb_{}", index), color_image, Default::default());
                        item.thumbnail_texture = Some(tex);
                    }
                }
                GuiEvent::BatchItemProgress { index, status, completed_count } => {
                    if let Some(item) = self.batch_items.get_mut(index) {
                        item.status = status;
                    }
                    self.batch_completed = completed_count;
                }
                GuiEvent::BatchFinished {
                    total,
                    success_count,
                    error_count,
                    duration_secs,
                } => {
                    let fps = total as f64 / duration_secs.max(0.001);
                    self.batch_fps = Some(fps);
                    let summary = format!(
                        "🎉 Finished in {:.2}s! ({} Succeeded, {} Failed) • Speed: {:.1} FPS",
                        duration_secs, success_count, error_count, fps
                    );
                    self.status_message = summary.clone();
                    self.batch_summary = Some(summary);
                }
            }
        }
    }
}

impl eframe::App for UpscalerGuiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // ==========================================
        // PRODUCT ACTIVATION LOCK SCREEN
        // ==========================================
        if !self.is_activated {
            egui::CentralPanel::default()
                .frame(egui::Frame::none().fill(egui::Color32::from_rgb(12, 15, 22)))
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(80.0);

                        egui::Frame::group(ui.style())
                            .fill(egui::Color32::from_rgb(18, 24, 34))
                            .stroke(egui::Stroke::new(1.5, egui::Color32::from_rgb(0, 230, 150)))
                            .rounding(14.0)
                            .inner_margin(egui::Margin::same(28.0))
                            .show(ui, |ui| {
                                ui.set_width(450.0);
                                ui.vertical_centered(|ui| {
                                    ui.label(
                                        egui::RichText::new("🔒 PRODUCT ACTIVATION")
                                            .color(egui::Color32::from_rgb(0, 230, 150))
                                            .size(24.0)
                                            .strong(),
                                    );
                                    ui.add_space(4.0);
                                    ui.label(
                                        egui::RichText::new("⚡ Image Upscale Pro (Mahmudul Hasan)")
                                            .color(egui::Color32::from_rgb(80, 200, 255))
                                            .size(13.0),
                                    );

                                    ui.add_space(16.0);
                                    ui.separator();
                                    ui.add_space(14.0);

                                    ui.label(
                                        egui::RichText::new("Please enter a Master Activation Password to unlock the application:")
                                            .color(egui::Color32::from_rgb(180, 195, 215))
                                            .size(13.0),
                                    );

                                    ui.add_space(14.0);

                                    let password_edit = ui.add(
                                        egui::TextEdit::singleline(&mut self.activation_input)
                                            .password(true)
                                            .hint_text("Enter Master Key...")
                                            .desired_width(320.0)
                                    );

                                    ui.add_space(14.0);

                                    if let Some(err) = &self.activation_error {
                                        ui.label(egui::RichText::new(err).color(egui::Color32::LIGHT_RED).strong().size(13.0));
                                        ui.add_space(8.0);
                                    }

                                    let activate_btn = ui.add(
                                        egui::Button::new(
                                            egui::RichText::new("🔓 ACTIVATE PRODUCT")
                                                .size(15.0)
                                                .color(egui::Color32::BLACK)
                                                .strong(),
                                        )
                                        .fill(egui::Color32::from_rgb(0, 230, 150))
                                        .min_size(egui::vec2(220.0, 42.0))
                                        .rounding(8.0),
                                    );

                                    let trigger_activate = activate_btn.clicked() || (password_edit.lost_focus() && ctx.input(|i| i.key_pressed(egui::Key::Enter)));

                                    if trigger_activate {
                                        match crate::license::activate_product(&self.activation_input) {
                                            Ok(_) => {
                                                self.is_activated = true;
                                                self.activation_error = None;
                                                self.status_message = "✅ Product successfully activated! Welcome to Image Upscale Pro.".to_string();
                                            }
                                            Err(e) => {
                                                self.activation_error = Some(format!("❌ {}", e));
                                            }
                                        }
                                    }

                                    ui.add_space(18.0);
                                    ui.separator();
                                    ui.add_space(8.0);
                                    ui.label(egui::RichText::new("One-time machine activation • Developed by Mahmudul Hasan").color(egui::Color32::GRAY).small());
                                });
                            });
                    });
                });
            return;
        }

        self.check_background_events(ctx);

        if self.batch_is_running.load(Ordering::SeqCst) {
            ctx.request_repaint();
        }

        // ==========================================
        // TOP HEADER BAR
        // ==========================================
        egui::TopBottomPanel::top("header_panel")
            .frame(egui::Frame::none().fill(egui::Color32::from_rgb(14, 17, 23)).inner_margin(egui::Margin::symmetric(14.0, 10.0)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    // Logo & App Name
                    ui.label(
                        egui::RichText::new("⚡ IMAGE UPSCALE")
                            .color(egui::Color32::from_rgb(0, 230, 150))
                            .size(18.0)
                            .strong(),
                    );
                    ui.label(
                        egui::RichText::new("PRO")
                            .color(egui::Color32::from_rgb(80, 200, 255))
                            .size(12.0)
                            .strong(),
                    );

                    ui.add_space(24.0);

                    // Segmented Mode Tabs
                    ui.horizontal(|ui| {
                        let single_selected = self.current_tab == AppTab::Single;
                        let single_btn = ui.add(
                            egui::Button::new(
                                egui::RichText::new("🖼️ Single Image Enhancer")
                                    .size(13.0)
                                    .color(if single_selected { egui::Color32::BLACK } else { egui::Color32::LIGHT_GRAY })
                                    .strong(),
                            )
                            .fill(if single_selected { egui::Color32::from_rgb(0, 230, 150) } else { egui::Color32::from_rgb(26, 32, 44) })
                            .rounding(6.0),
                        );
                        if single_btn.clicked() {
                            self.current_tab = AppTab::Single;
                        }

                        let batch_selected = self.current_tab == AppTab::Batch;
                        let batch_btn = ui.add(
                            egui::Button::new(
                                egui::RichText::new(format!("📚 Batch Gallery & Queue ({})", self.batch_items.len()))
                                    .size(13.0)
                                    .color(if batch_selected { egui::Color32::BLACK } else { egui::Color32::LIGHT_GRAY })
                                    .strong(),
                            )
                            .fill(if batch_selected { egui::Color32::from_rgb(0, 230, 150) } else { egui::Color32::from_rgb(26, 32, 44) })
                            .rounding(6.0),
                        );
                        if batch_btn.clicked() {
                            self.current_tab = AppTab::Batch;
                        }
                    });

                    // Right Side Target Badge & About Button
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(egui::RichText::new("ℹ️ About Developer").size(12.0).color(egui::Color32::from_rgb(0, 230, 150))).clicked() {
                            self.show_about_dialog = true;
                        }

                        ui.add_space(10.0);

                        ui.label(
                            egui::RichText::new(format!("🎯 Target: {}x{} px", self.target_width, self.target_height))
                                .color(egui::Color32::from_rgb(80, 200, 255))
                                .strong()
                                .size(13.0),
                        );
                    });
                });
            });

        // ==========================================
        // BOTTOM STATUS BAR
        // ==========================================
        egui::TopBottomPanel::bottom("status_bar")
            .frame(egui::Frame::none().fill(egui::Color32::from_rgb(12, 15, 20)).inner_margin(egui::Margin::symmetric(14.0, 8.0)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(&self.status_message)
                            .color(if self.status_message.contains("Error") || self.status_message.contains("❌") || self.status_message.contains("Failed") {
                                egui::Color32::LIGHT_RED
                            } else if self.status_message.contains("✨") || self.status_message.contains("🎉") {
                                egui::Color32::from_rgb(0, 230, 150)
                            } else {
                                egui::Color32::from_rgb(160, 180, 200)
                            })
                            .size(13.0),
                    );

                    if self.current_tab == AppTab::Single {
                        if let Some(ms) = self.single_last_ms {
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let fps = 1000.0 / ms.max(0.001);
                                ui.label(
                                    egui::RichText::new(format!("⚡ {:.1} FPS ({:.2} ms)", fps, ms))
                                        .color(egui::Color32::YELLOW)
                                        .strong(),
                                );
                            });
                        }
                    } else if let Some(fps) = self.batch_fps {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                egui::RichText::new(format!("⚡ Batch Speed: {:.1} FPS", fps))
                                    .color(egui::Color32::YELLOW)
                                    .strong(),
                            );
                        });
                    }
                });
            });

        // ==========================================
        // LEFT SIDEBAR: CONTROLS & SETTINGS
        // ==========================================
        egui::SidePanel::left("controls_panel")
            .resizable(false)
            .default_width(340.0)
            .frame(egui::Frame::none().fill(egui::Color32::from_rgb(18, 22, 30)).inner_margin(egui::Margin::same(14.0)))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    // Source Input Section
                    ui.label(egui::RichText::new("📂 SOURCE INPUT").color(egui::Color32::from_rgb(130, 150, 180)).size(11.0).strong());
                    ui.add_space(4.0);

                    egui::Frame::group(ui.style())
                        .fill(egui::Color32::from_rgb(24, 30, 42))
                        .rounding(8.0)
                        .inner_margin(egui::Margin::same(10.0))
                        .show(ui, |ui| {
                            match self.current_tab {
                                AppTab::Single => {
                                    ui.horizontal(|ui| {
                                        if ui.button(egui::RichText::new("📂 Choose Single Image...").strong()).clicked() {
                                            self.pick_single_image();
                                        }

                                        if self.single_input_image.is_some() {
                                            if ui.button("❌ Clear").clicked() {
                                                self.single_input_image = None;
                                                self.single_input_texture = None;
                                                self.single_processed_image = None;
                                                self.single_processed_texture = None;
                                                self.single_input_path = None;
                                                self.status_message = "Ready.".to_string();
                                            }
                                        }
                                    });

                                    if let Some(img) = self.single_input_image.as_ref() {
                                        ui.add_space(4.0);
                                        ui.label(egui::RichText::new(format!("Original: {} x {} px", img.width(), img.height())).color(egui::Color32::LIGHT_GREEN).small());
                                    } else {
                                        ui.add_space(2.0);
                                        ui.label(egui::RichText::new("No image loaded").italics().color(egui::Color32::GRAY).small());
                                    }
                                }
                                AppTab::Batch => {
                                    ui.horizontal(|ui| {
                                        if ui.button(egui::RichText::new("📁 Select Folder...").strong()).clicked() {
                                            self.pick_batch_folder();
                                        }
                                        if ui.button(egui::RichText::new("🖼️ Multi-Files...").strong()).clicked() {
                                            self.pick_multiple_files();
                                        }
                                    });

                                    ui.add_space(4.0);
                                    ui.label(egui::RichText::new(&self.batch_source_desc).color(egui::Color32::LIGHT_BLUE).small());
                                }
                            }
                        });

                    ui.add_space(14.0);

                    // Target Resolution Section
                    ui.label(egui::RichText::new("🎯 TARGET RESOLUTION").color(egui::Color32::from_rgb(130, 150, 180)).size(11.0).strong());
                    ui.add_space(4.0);

                    egui::Frame::group(ui.style())
                        .fill(egui::Color32::from_rgb(24, 30, 42))
                        .rounding(8.0)
                        .inner_margin(egui::Margin::same(10.0))
                        .show(ui, |ui| {
                            // Quick Presets Pills
                            ui.horizontal_wrapped(|ui| {
                                let is_default = self.target_width == 720 && self.target_height == 480;
                                let def_btn = ui.add(
                                    egui::Button::new(egui::RichText::new("720x480 (Default)").strong().size(12.0))
                                        .fill(if is_default { egui::Color32::from_rgb(0, 180, 110) } else { egui::Color32::from_rgb(35, 45, 62) })
                                        .rounding(4.0),
                                );
                                if def_btn.clicked() {
                                    self.target_width = 720;
                                    self.target_height = 480;
                                }

                                if ui.button(egui::RichText::new("1080p FHD").size(12.0)).clicked() {
                                    self.target_width = 1920;
                                    self.target_height = 1080;
                                }

                                if ui.button(egui::RichText::new("720p HD").size(12.0)).clicked() {
                                    self.target_width = 1280;
                                    self.target_height = 720;
                                }

                                if ui.button(egui::RichText::new("4K UHD").size(12.0)).clicked() {
                                    self.target_width = 3840;
                                    self.target_height = 2160;
                                }
                            });

                            ui.add_space(8.0);

                            // Width & Height inputs
                            ui.horizontal(|ui| {
                                ui.label("Width :");
                                let w_drag = ui.add(egui::DragValue::new(&mut self.target_width).range(16..=16384).speed(2));
                                if w_drag.changed() && self.lock_aspect_ratio {
                                    self.target_height = ((self.target_width as f64) / self.saved_aspect_ratio).round() as u32;
                                }

                                ui.label("Height:");
                                let h_drag = ui.add(egui::DragValue::new(&mut self.target_height).range(16..=16384).speed(2));
                                if h_drag.changed() && self.lock_aspect_ratio {
                                    self.target_width = ((self.target_height as f64) * self.saved_aspect_ratio).round() as u32;
                                }
                            });

                            ui.horizontal(|ui| {
                                ui.checkbox(&mut self.lock_aspect_ratio, "Lock Aspect Ratio");
                                if self.current_tab == AppTab::Single {
                                    if let Some(img) = self.single_input_image.as_ref() {
                                        if ui.button("2x").clicked() {
                                            self.target_width = img.width() * 2;
                                            self.target_height = img.height() * 2;
                                        }
                                        if ui.button("4x").clicked() {
                                            self.target_width = img.width() * 4;
                                            self.target_height = img.height() * 4;
                                        }
                                    }
                                }
                            });
                        });

                    ui.add_space(14.0);

                    // Enhancement & Scaling Options
                    ui.label(egui::RichText::new("⚙️ QUALITY & SCALING ENGINE").color(egui::Color32::from_rgb(130, 150, 180)).size(11.0).strong());
                    ui.add_space(4.0);

                    egui::Frame::group(ui.style())
                        .fill(egui::Color32::from_rgb(24, 30, 42))
                        .rounding(8.0)
                        .inner_margin(egui::Margin::same(10.0))
                        .show(ui, |ui| {
                            // Aspect Mode
                            egui::ComboBox::from_label("Aspect Mode")
                                .selected_text(match self.mode {
                                    ScaleMode::Stretch => "Stretch (Exact 720x480)",
                                    ScaleMode::Fit => "Fit (Letterbox/Pad)",
                                    ScaleMode::Fill => "Fill (Smart Crop)",
                                })
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut self.mode, ScaleMode::Stretch, "Stretch (Exact 720x480)");
                                    ui.selectable_value(&mut self.mode, ScaleMode::Fit, "Fit (Letterbox/Pad)");
                                    ui.selectable_value(&mut self.mode, ScaleMode::Fill, "Fill (Smart Crop)");
                                });

                            ui.add_space(4.0);

                            // Filter
                            egui::ComboBox::from_label("Filter")
                                .selected_text(match self.filter {
                                    ResampleFilter::Lanczos3 => "Lanczos3 (High Quality)",
                                    ResampleFilter::CatmullRom => "Catmull-Rom (Fast)",
                                    ResampleFilter::Mitchell => "Mitchell (Smooth)",
                                    ResampleFilter::Bilinear => "Bilinear (Fast)",
                                    ResampleFilter::Nearest => "Nearest (Retro)",
                                })
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut self.filter, ResampleFilter::Lanczos3, "Lanczos3 (High Quality)");
                                    ui.selectable_value(&mut self.filter, ResampleFilter::CatmullRom, "Catmull-Rom (Fast)");
                                    ui.selectable_value(&mut self.filter, ResampleFilter::Mitchell, "Mitchell (Smooth)");
                                    ui.selectable_value(&mut self.filter, ResampleFilter::Bilinear, "Bilinear (Fast)");
                                    ui.selectable_value(&mut self.filter, ResampleFilter::Nearest, "Nearest (Retro)");
                                });

                            ui.add_space(6.0);
                            ui.label(egui::RichText::new(format!("CAS Sharpening: {:.2}", self.sharpen)).color(egui::Color32::LIGHT_GREEN).small());
                            ui.add(egui::Slider::new(&mut self.sharpen, 0.0..=1.0).step_by(0.05));

                            ui.label(egui::RichText::new(format!("JPEG/WebP Quality: {}%", self.quality)).small());
                            ui.add(egui::Slider::new(&mut self.quality, 10..=100));
                        });

                    ui.add_space(14.0);

                    // Destination Folder (For Batch Mode or Quick Single)
                    ui.label(egui::RichText::new("📁 OUTPUT DESTINATION").color(egui::Color32::from_rgb(130, 150, 180)).size(11.0).strong());
                    ui.add_space(4.0);

                    egui::Frame::group(ui.style())
                        .fill(egui::Color32::from_rgb(24, 30, 42))
                        .rounding(8.0)
                        .inner_margin(egui::Margin::same(10.0))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                if ui.button("📂 Browse Folder...").clicked() {
                                    self.pick_batch_output_folder();
                                }
                                if ui.button("📂 Open Folder in OS").clicked() {
                                    self.open_output_folder();
                                }
                            });

                            if let Some(out) = &self.batch_output_dir {
                                ui.add_space(4.0);
                                ui.label(egui::RichText::new(format!("{}", out.display())).color(egui::Color32::LIGHT_YELLOW).small());
                            } else {
                                ui.add_space(4.0);
                                ui.label(egui::RichText::new("Default: ./upscaled_outputs").italics().color(egui::Color32::GRAY).small());
                            }
                        });

                    ui.add_space(16.0);

                    // Primary Action Button
                    match self.current_tab {
                        AppTab::Single => {
                            let has_input = self.single_input_image.is_some() && !self.is_processing_single;
                            let has_processed = self.single_processed_image.is_some() && !self.is_processing_single;

                            ui.vertical_centered_justified(|ui| {
                                let upscale_btn = ui.add_enabled(
                                    has_input,
                                    egui::Button::new(
                                        egui::RichText::new(if self.is_processing_single { "⏳ PROCESSING..." } else { "⚡ UPSCALE & ENHANCE" })
                                            .size(15.0)
                                            .color(egui::Color32::BLACK)
                                            .strong(),
                                    )
                                    .fill(egui::Color32::from_rgb(0, 230, 150))
                                    .min_size(egui::vec2(0.0, 42.0))
                                    .rounding(8.0),
                                );

                                if upscale_btn.clicked() {
                                    self.upscale_single();
                                }

                                ui.add_space(8.0);

                                let save_btn = ui.add_enabled(
                                    has_processed,
                                    egui::Button::new(
                                        egui::RichText::new("💾 Save Output Image...")
                                            .size(14.0)
                                            .strong(),
                                    )
                                    .min_size(egui::vec2(0.0, 36.0))
                                    .rounding(6.0),
                                );

                                if save_btn.clicked() {
                                    self.save_single_image();
                                }
                            });
                        }
                        AppTab::Batch => {
                            let is_running = self.batch_is_running.load(Ordering::SeqCst);
                            let has_files = !self.batch_items.is_empty();

                            ui.vertical_centered_justified(|ui| {
                                if !is_running {
                                    let start_btn = ui.add_enabled(
                                        has_files,
                                        egui::Button::new(
                                            egui::RichText::new(format!("🚀 START BATCH ({} IMAGES)", self.batch_items.len()))
                                                .size(15.0)
                                                .color(egui::Color32::BLACK)
                                                .strong(),
                                        )
                                        .fill(egui::Color32::from_rgb(0, 230, 150))
                                        .min_size(egui::vec2(0.0, 44.0))
                                        .rounding(8.0),
                                    );

                                    if start_btn.clicked() {
                                        self.start_batch_processing();
                                    }
                                } else {
                                    let stop_btn = ui.add(
                                        egui::Button::new(
                                            egui::RichText::new("⏹ STOP BATCH")
                                                .size(15.0)
                                                .color(egui::Color32::WHITE)
                                                .strong(),
                                        )
                                        .fill(egui::Color32::from_rgb(220, 50, 50))
                                        .min_size(egui::vec2(0.0, 44.0))
                                        .rounding(8.0),
                                    );

                                    if stop_btn.clicked() {
                                        self.batch_is_running.store(false, Ordering::SeqCst);
                                    }
                                }
                            });
                        }
                    }

                    ui.add_space(18.0);
                    ui.separator();
                    ui.add_space(4.0);
                    ui.vertical_centered(|ui| {
                        if ui.small_button(egui::RichText::new("🦀 Built with Rust by Mahmudul Hasan").color(egui::Color32::from_rgb(120, 150, 190)).small()).clicked() {
                            self.show_about_dialog = true;
                        }
                    });
                });
            });

        // ==========================================
        // MAIN WORKSPACE VIEWPORT
        // ==========================================
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(egui::Color32::from_rgb(12, 15, 20)).inner_margin(egui::Margin::same(16.0)))
            .show(ctx, |ui| {
                match self.current_tab {
                    // --- SINGLE IMAGE WORKSPACE ---
                    AppTab::Single => {
                        ui.horizontal(|ui| {
                            ui.heading(egui::RichText::new("Single Image Inspection & Compare").size(18.0).strong());
                            if let Some(ms) = self.single_last_ms {
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.label(
                                        egui::RichText::new(format!("⚡ {:.2} ms per image", ms))
                                            .color(egui::Color32::from_rgb(0, 230, 150))
                                            .strong(),
                                    );
                                });
                            }
                        });

                        ui.separator();
                        ui.add_space(8.0);

                        egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
                            ui.horizontal_top(|ui| {
                                // Source View Card
                                egui::Frame::group(ui.style())
                                    .fill(egui::Color32::from_rgb(18, 22, 30))
                                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(35, 45, 60)))
                                    .rounding(10.0)
                                    .inner_margin(egui::Margin::same(12.0))
                                    .show(ui, |ui| {
                                        ui.vertical(|ui| {
                                            ui.horizontal(|ui| {
                                                ui.label(egui::RichText::new("📷 Source Image").strong().size(15.0));
                                                if let Some(img) = self.single_input_image.as_ref() {
                                                    ui.label(egui::RichText::new(format!("{}x{} px", img.width(), img.height())).color(egui::Color32::LIGHT_BLUE).strong());
                                                }
                                            });
                                            ui.separator();
                                            ui.add_space(6.0);

                                            if let Some(texture) = self.single_input_texture.as_ref() {
                                                let size = texture.size_vec2();
                                                let aspect = size.x / size.y.max(1.0);
                                                let display_w = 440.0;
                                                let display_h = display_w / aspect;
                                                ui.image((texture.id(), egui::vec2(display_w, display_h)));
                                            } else {
                                                let (rect, _resp) = ui.allocate_exact_size(egui::vec2(440.0, 320.0), egui::Sense::hover());
                                                ui.painter().rect_filled(rect, 8.0, egui::Color32::from_rgb(24, 30, 42));
                                                let mut child_ui = ui.new_child(egui::UiBuilder::new().max_rect(rect));
                                                child_ui.vertical_centered(|ui| {
                                                    ui.add_space(110.0);
                                                    ui.label(egui::RichText::new("No Source Image Loaded").color(egui::Color32::GRAY).strong().size(16.0));
                                                    ui.add_space(8.0);
                                                    if ui.button("📂 Click to Choose Image").clicked() {
                                                        self.pick_single_image();
                                                    }
                                                });
                                            }
                                        });
                                    });

                                ui.add_space(16.0);

                                // Upscaled Result Card
                                egui::Frame::group(ui.style())
                                    .fill(egui::Color32::from_rgb(18, 22, 30))
                                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(0, 180, 110)))
                                    .rounding(10.0)
                                    .inner_margin(egui::Margin::same(12.0))
                                    .show(ui, |ui| {
                                        ui.vertical(|ui| {
                                            ui.horizontal(|ui| {
                                                ui.label(
                                                    egui::RichText::new("✨ Upscaled Result")
                                                        .strong()
                                                        .size(15.0)
                                                        .color(egui::Color32::from_rgb(0, 230, 150)),
                                                );
                                                ui.label(
                                                    egui::RichText::new(format!("Target: {}x{} px", self.target_width, self.target_height))
                                                        .color(egui::Color32::LIGHT_GREEN)
                                                        .strong(),
                                                );
                                            });
                                            ui.separator();
                                            ui.add_space(6.0);

                                            if let Some(texture) = self.single_processed_texture.as_ref() {
                                                let size = texture.size_vec2();
                                                let aspect = size.x / size.y.max(1.0);
                                                let display_w = 440.0;
                                                let display_h = display_w / aspect;
                                                ui.image((texture.id(), egui::vec2(display_w, display_h)));
                                            } else {
                                                let (rect, _resp) = ui.allocate_exact_size(egui::vec2(440.0, 320.0), egui::Sense::hover());
                                                ui.painter().rect_filled(rect, 8.0, egui::Color32::from_rgb(24, 30, 42));
                                                let mut child_ui = ui.new_child(egui::UiBuilder::new().max_rect(rect));
                                                child_ui.vertical_centered(|ui| {
                                                    ui.add_space(120.0);
                                                    ui.label(egui::RichText::new("Upscaled Preview").color(egui::Color32::GRAY).strong().size(16.0));
                                                    ui.add_space(6.0);
                                                    ui.label("Click '⚡ UPSCALE & ENHANCE' on left");
                                                });
                                            }
                                        });
                                    });
                            });
                        });
                    }

                    // --- BATCH PROCESSING WORKSPACE ---
                    AppTab::Batch => {
                        // Top Stats Ribbon Card
                        egui::Frame::group(ui.style())
                            .fill(egui::Color32::from_rgb(18, 22, 30))
                            .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(35, 45, 60)))
                            .rounding(10.0)
                            .inner_margin(egui::Margin::same(12.0))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(
                                        egui::RichText::new(format!("📋 Batch Queue: {} Images", self.batch_items.len()))
                                            .strong()
                                            .size(16.0),
                                    );

                                    ui.add_space(16.0);

                                    if !self.batch_items.is_empty() {
                                        if ui.button("🗑️ Clear Queue").clicked() {
                                            self.batch_items.clear();
                                            self.batch_total = 0;
                                            self.batch_completed = 0;
                                            self.batch_summary = None;
                                            self.batch_source_desc = "Queue cleared.".to_string();
                                        }
                                    }

                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        if ui.button("📂 Open Output Folder").clicked() {
                                            self.open_output_folder();
                                        }
                                    });
                                });

                                ui.add_space(8.0);

                                let fraction = if self.batch_total > 0 {
                                    self.batch_completed as f32 / self.batch_total as f32
                                } else {
                                    0.0
                                };

                                ui.add(
                                    egui::ProgressBar::new(fraction)
                                        .text(format!("{}/{} completed ({:.0}%)", self.batch_completed, self.batch_total, fraction * 100.0))
                                        .animate(self.batch_is_running.load(Ordering::SeqCst)),
                                );

                                if let Some(summary) = &self.batch_summary {
                                    ui.add_space(6.0);
                                    ui.label(egui::RichText::new(summary).color(egui::Color32::from_rgb(0, 230, 150)).strong());
                                }
                            });

                        ui.add_space(14.0);
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("🖼️ THUMBNAIL GALLERY & LIVE STATUS").color(egui::Color32::from_rgb(130, 150, 180)).size(12.0).strong());
                            ui.label(egui::RichText::new("(Click any image card to inspect)").color(egui::Color32::GRAY).small());
                        });
                        ui.add_space(6.0);

                        // Thumbnail Cards Grid
                        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                            if self.batch_items.is_empty() {
                                ui.vertical_centered(|ui| {
                                    ui.add_space(80.0);
                                    ui.label(egui::RichText::new("No images in batch queue").color(egui::Color32::GRAY).size(18.0));
                                    ui.add_space(10.0);
                                    ui.label("Click '📁 Select Folder...' or '🖼️ Multi-Files...' on the left panel to load images.");
                                });
                            } else {
                                let mut to_remove = None;
                                let mut to_inspect = None;

                                ui.horizontal_wrapped(|ui| {
                                    for (idx, item) in self.batch_items.iter().enumerate() {
                                        let card_resp = egui::Frame::group(ui.style())
                                            .fill(egui::Color32::from_rgb(20, 25, 35))
                                            .stroke(egui::Stroke::new(
                                                1.5,
                                                match &item.status {
                                                    BatchStatus::Pending => egui::Color32::from_rgb(45, 58, 78),
                                                    BatchStatus::Processing => egui::Color32::YELLOW,
                                                    BatchStatus::Completed(_) => egui::Color32::from_rgb(0, 230, 150),
                                                    BatchStatus::Failed(_) => egui::Color32::LIGHT_RED,
                                                },
                                            ))
                                            .rounding(10.0)
                                            .show(ui, |ui| {
                                                ui.set_width(175.0);
                                                ui.set_height(220.0);
                                                ui.vertical_centered(|ui| {
                                                    // Top item header with remove button
                                                    ui.horizontal(|ui| {
                                                        ui.label(egui::RichText::new(format!("#{}", idx + 1)).color(egui::Color32::GRAY).small());
                                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                            if ui.small_button("✖").clicked() {
                                                                to_remove = Some(idx);
                                                            }
                                                        });
                                                    });

                                                    ui.add_space(2.0);

                                                    // Thumbnail Picture View
                                                    if let Some(texture) = item.thumbnail_texture.as_ref() {
                                                        let size = texture.size_vec2();
                                                        let aspect = size.x / size.y.max(1.0);
                                                        let thumb_w = 155.0;
                                                        let thumb_h = (thumb_w / aspect).min(115.0);
                                                        let img_resp = ui.image((texture.id(), egui::vec2(thumb_w, thumb_h)));
                                                        if img_resp.clicked() {
                                                            to_inspect = Some(item.path.clone());
                                                        }
                                                    } else {
                                                        let (rect, _resp) = ui.allocate_exact_size(egui::vec2(155.0, 100.0), egui::Sense::hover());
                                                        ui.painter().rect_filled(rect, 4.0, egui::Color32::from_rgb(30, 38, 50));
                                                        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
                                                        child.vertical_centered(|ui| {
                                                            ui.add_space(38.0);
                                                            ui.label(egui::RichText::new("📷 Loading...").color(egui::Color32::GRAY).small());
                                                        });
                                                    }

                                                    ui.add_space(4.0);

                                                    // Name
                                                    let display_name = if item.name.len() > 17 {
                                                        format!("{}...", &item.name[..14])
                                                    } else {
                                                        item.name.clone()
                                                    };
                                                    ui.label(egui::RichText::new(display_name).strong().size(12.0));

                                                    // Dimensions
                                                    if let Some((w, h)) = item.dimensions {
                                                        ui.label(egui::RichText::new(format!("{}x{} px", w, h)).color(egui::Color32::LIGHT_BLUE).small());
                                                    } else {
                                                        ui.label(egui::RichText::new("...").small());
                                                    }

                                                    ui.add_space(3.0);

                                                    // Status Pill
                                                    match &item.status {
                                                        BatchStatus::Pending => {
                                                            ui.label(egui::RichText::new("⏳ Pending").color(egui::Color32::GRAY).small());
                                                        }
                                                        BatchStatus::Processing => {
                                                            ui.label(egui::RichText::new("🔄 Upscaling...").color(egui::Color32::YELLOW).strong().small());
                                                        }
                                                        BatchStatus::Completed(_) => {
                                                            ui.label(egui::RichText::new("✅ 720x480 Done").color(egui::Color32::from_rgb(0, 230, 150)).strong().small());
                                                        }
                                                        BatchStatus::Failed(_) => {
                                                            ui.label(egui::RichText::new("❌ Failed").color(egui::Color32::LIGHT_RED).small());
                                                        }
                                                    }
                                                });
                                            });

                                        if card_resp.response.clicked() {
                                            to_inspect = Some(item.path.clone());
                                        }

                                        ui.add_space(8.0);
                                    }
                                });

                                if let Some(idx) = to_remove {
                                    self.batch_items.remove(idx);
                                    self.batch_total = self.batch_items.len();
                                }

                                if let Some(path) = to_inspect {
                                    if let Ok(dyn_img) = image::open(&path) {
                                        let rgba = dyn_img.to_rgba8();
                                        let color_image = egui::ColorImage::from_rgba_unmultiplied(
                                            [dyn_img.width() as usize, dyn_img.height() as usize],
                                            rgba.as_raw(),
                                        );
                                        let texture = ctx.load_texture("input_preview", color_image, Default::default());
                                        self.single_input_texture = Some(texture);
                                        self.single_input_image = Some(dyn_img);
                                        self.single_input_path = Some(path);
                                        self.single_processed_image = None;
                                        self.single_processed_texture = None;
                                        self.current_tab = AppTab::Single;
                                    }
                                }
                            }
                        });
                    }
                }
            });

        // ==========================================
        // DEVELOPER ABOUT MODAL DIALOG
        // ==========================================
        if self.show_about_dialog {
            egui::Window::new("ℹ️ About Developer")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .frame(
                    egui::Frame::window(ctx.style().as_ref())
                        .fill(egui::Color32::from_rgb(18, 22, 32))
                        .stroke(egui::Stroke::new(1.5, egui::Color32::from_rgb(0, 230, 150)))
                        .rounding(12.0)
                        .inner_margin(egui::Margin::same(20.0)),
                )
                .show(ctx, |ui| {
                    ui.set_width(380.0);
                    ui.vertical_centered(|ui| {
                        ui.heading(
                            egui::RichText::new("⚡ Image Upscale Pro")
                                .color(egui::Color32::from_rgb(0, 230, 150))
                                .size(22.0)
                                .strong(),
                        );
                        ui.label(egui::RichText::new("v0.1.0 • High-Speed Edition").color(egui::Color32::from_rgb(80, 200, 255)).small());

                        ui.add_space(14.0);
                        ui.separator();
                        ui.add_space(10.0);

                        ui.label(
                            egui::RichText::new("👨‍💻 Developed by Mahmudul Hasan")
                                .color(egui::Color32::WHITE)
                                .size(16.0)
                                .strong(),
                        );

                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new("A modern, blazing-fast image upscaler and quality enhancement tool built with pure Rust for Linux & Windows.")
                                .color(egui::Color32::from_rgb(180, 195, 215))
                                .size(13.0),
                        );

                        ui.add_space(14.0);
                        ui.horizontal_wrapped(|ui| {
                            let badge = |ui: &mut egui::Ui, text: &str, color: egui::Color32| {
                                egui::Frame::none()
                                    .fill(egui::Color32::from_rgb(26, 33, 48))
                                    .stroke(egui::Stroke::new(1.0, color))
                                    .rounding(4.0)
                                    .inner_margin(egui::Margin::symmetric(8.0, 4.0))
                                    .show(ui, |ui| {
                                        ui.label(egui::RichText::new(text).color(color).size(11.0).strong());
                                    });
                            };

                            badge(ui, "🦀 Pure Rust", egui::Color32::from_rgb(255, 140, 60));
                            badge(ui, "⚡ Hardware SIMD", egui::Color32::from_rgb(0, 230, 150));
                            badge(ui, "🚀 Rayon Parallel", egui::Color32::from_rgb(80, 200, 255));
                            badge(ui, "🖥️ egui GUI", egui::Color32::from_rgb(220, 120, 255));
                        });

                        ui.add_space(18.0);
                        if ui.button(egui::RichText::new("  Close  ").strong()).clicked() {
                            self.show_about_dialog = false;
                        }
                    });
                });
        }
    }
}
