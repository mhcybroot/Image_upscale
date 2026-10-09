use image::{ImageBuffer, Rgb};
use std::fs;

fn main() {
    fs::create_dir_all("test_inputs").unwrap();
    for i in 0..10 {
        let mut img = ImageBuffer::new(640, 480);
        for (x, y, pixel) in img.enumerate_pixels_mut() {
            let r = ((x * 255) / 640) as u8;
            let g = ((y * 255) / 480) as u8;
            let b = ((i * 25) % 255) as u8;
            *pixel = Rgb([r, g, b]);
        }
        img.save(format!("test_inputs/sample_{}.png", i + 1)).unwrap();
    }
    println!("Generated 10 test images (640x480) in test_inputs/");
}
