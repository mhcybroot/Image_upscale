# ⚡ Fast SIMD Image Upscaler & Quality Enhancer

A blazing-fast image upscaler and quality enhancement tool built in Rust. It supports arbitrary image resolutions, provides default upscaling from **640x480 to 720x480**, is fully customizable by the user, and runs seamlessly on both **Linux and Windows**.

---

## 🚀 Features

- **Blazing Fast Speed**: Uses hardware SIMD instructions (AVX2/SSE4.1/NEON) and multi-threaded CPU parallelization (`Rayon` + `fast_image_resize`) achieving **150+ FPS** batch processing.
- **Default Resolution**: Upscales from any source (e.g. `640x480`) to **`720x480`** by default.
- **Customizable Resolutions & Modes**: Easily override target width/height or use presets (`fit`, `stretch`, `fill`).
- **Contrast-Adaptive Sharpening (CAS)**: Enhances visual edges, textures, and picture clarity while preventing haloing or artifacts.
## 🔐 Product Activation & Master Keys

This application requires a **One-Time Machine Activation**. Once activated with either of the 2 master passwords, the license is permanently stored locally and will never ask again.

### 🔑 The 2 Master Passwords:
1. `MAHMUDUL-PRO-2026-X9`
2. `CYBROOT-SUPER-SCALE-99`

---

## 🖥️ Graphical User Interface (GUI)

You can launch the modern native desktop GUI in any of the following ways:

```bash
# Launch GUI directly
./target/release/gui

# OR launch via main binary
./target/release/image_upscale --gui
# OR simply run without arguments in your desktop environment
./target/release/image_upscale
```

### GUI Features:
- **Drag & Drop**: Simply drop any image onto the window.
- **Native File Dialog**: Click `📂 Choose Image...` or `💾 Save Output Image...`.
- **One-Click Presets**:
  - `720x480 (Default NTSC)`
  - `1080p (Full HD)`
  - `720p (HD)`
  - `4K (Ultra HD)`
  - `2x Scale` / `4x Scale`
- **Custom Width & Height**: Adjust precise pixel dimensions.
- **Aspect Ratio Controls**: Choose `Stretch (Exact Size)`, `Fit (Letterbox/Pad)`, or `Fill (Smart Crop)`.
- **Quality Enhancement**: Dynamic **CAS (Contrast-Adaptive Sharpening)** slider (`0.0` to `1.0`).
- **Interactive Before/After Viewport**: Side-by-side comparison with real-time speed/FPS metrics.

## 👨‍💻 Developer & Credits

- **Author / Developer**: **Mahmudul Hasan**
- **Core Technology**: Pure Rust 🦀 (SIMD AVX2/SSE4.1 Resampling, Rayon Multithreading, Contrast-Adaptive Sharpening, egui Native Desktop GUI)
- **Supported Platforms**: Linux (`x86_64`) and Windows (`x86_64 .exe`)

---

## 📦 Build Instructions

### 1. Build for Linux
```bash
cargo build --release --bin image_upscale
```
The binary will be located at `target/release/image_upscale`.

### 2. Build for Windows (from Linux or Windows)
On Windows:
```cmd
cargo build --release --bin image_upscale
```
To cross-compile from Linux for Windows:
```bash
rustup target add x86_64-pc-windows-gnu
cargo build --release --target x86_64-pc-windows-gnu --bin image_upscale
```
Output: `target/x86_64-pc-windows-gnu/release/image_upscale.exe`

---

## 🛠 Usage Examples

### 1. Default Upscale (640x480 -> 720x480)
```bash
./target/release/image_upscale -i input.png -o output.png
```

### 2. Batch Process a Folder of Images
```bash
./target/release/image_upscale -i /path/to/input_folder -o /path/to/output_folder
```

### 3. Custom Resolution & Mode
```bash
# Upscale to 1920x1080 preserving aspect ratio with fit mode
./target/release/image_upscale -i input.jpg -o output.jpg -W 1920 -H 1080 --mode fit --sharpen 0.4
```

### 4. Use `config.toml`
```bash
./target/release/image_upscale --config config.toml -i input.png -o output.png
```

---

## ⚙️ CLI Options Reference

| Flag | Long Option | Description | Default |
|------|-------------|-------------|---------|
| `-i` | `--input` | Input image file or directory path | *(Required)* |
| `-o` | `--output` | Output image file or directory path | *(Required)* |
| `-W` | `--width` | Target width in pixels | `720` |
| `-H` | `--height` | Target height in pixels | `480` |
| `-m` | `--mode` | Scaling mode: `stretch`, `fit`, `fill` | `stretch` |
| `-f` | `--filter` | Filter: `lanczos3`, `catmull-rom`, `mitchell`, `bilinear`, `nearest` | `lanczos3` |
| `-s` | `--sharpen` | Adaptive sharpening strength (`0.0` to `1.0`) | `0.35` |
| `-q` | `--quality` | Output JPEG/WebP quality (`1-100`) | `95` |
| `-t` | `--threads` | Number of worker threads (`0` = all cores) | `0` |
|      | `--config` | Path to custom `config.toml` | `None` |
