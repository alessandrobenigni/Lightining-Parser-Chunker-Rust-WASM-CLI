use std::collections::HashMap;
use std::path::Path;

use crate::model::{Block, ElementType};

pub struct ImageParser;

impl super::FormatParser for ImageParser {
    fn parse(&self, path: &Path) -> Result<Vec<Block>, crate::Error> {
        let metadata =
            std::fs::metadata(path).map_err(|e| crate::Error::Io(e.to_string()))?;
        let file_size = metadata.len();

        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("unknown")
            .to_lowercase();

        // Try to read image dimensions from the file header
        let dimensions = read_image_dimensions(path, &ext);

        let dim_text = match dimensions {
            Some((w, h)) => format!("{w}x{h}"),
            None => "unknown dimensions".into(),
        };

        let text = format!(
            "Image ({ext}): {dim_text}, {file_size} bytes — OCR not available (ONNX runtime not configured)"
        );

        let mut meta = HashMap::new();
        meta.insert(
            "file_size".into(),
            serde_json::Value::Number(serde_json::Number::from(file_size)),
        );
        meta.insert("format".into(), serde_json::Value::String(ext.clone()));
        if let Some((w, h)) = dimensions {
            meta.insert(
                "width".into(),
                serde_json::Value::Number(serde_json::Number::from(w)),
            );
            meta.insert(
                "height".into(),
                serde_json::Value::Number(serde_json::Number::from(h)),
            );
        }

        Ok(vec![Block {
            element_type: ElementType::Image,
            text,
            page: 1,
            confidence: 0.0,
            metadata: meta,
            ..Block::default()
        }])
    }

    fn supported_extensions(&self) -> &[&str] {
        &["png", "jpg", "jpeg", "tiff", "tif", "bmp", "gif", "webp", "heic"]
    }
}

/// Read image dimensions from file header bytes (no external image crate needed).
fn read_image_dimensions(path: &Path, ext: &str) -> Option<(u32, u32)> {
    let data = std::fs::read(path).ok()?;
    match ext {
        "png" => read_png_dimensions(&data),
        "jpg" | "jpeg" => read_jpeg_dimensions(&data),
        "gif" => read_gif_dimensions(&data),
        "bmp" => read_bmp_dimensions(&data),
        _ => None,
    }
}

fn read_png_dimensions(data: &[u8]) -> Option<(u32, u32)> {
    // PNG: bytes 16-19 = width, 20-23 = height (big-endian in IHDR chunk)
    if data.len() < 24 || !data.starts_with(b"\x89PNG") {
        return None;
    }
    let w = u32::from_be_bytes([data[16], data[17], data[18], data[19]]);
    let h = u32::from_be_bytes([data[20], data[21], data[22], data[23]]);
    Some((w, h))
}

fn read_jpeg_dimensions(data: &[u8]) -> Option<(u32, u32)> {
    // JPEG: scan for SOF0 (0xFF 0xC0) marker
    if data.len() < 4 || data[0] != 0xFF || data[1] != 0xD8 {
        return None;
    }
    let mut i = 2;
    while i + 1 < data.len() {
        if data[i] != 0xFF {
            i += 1;
            continue;
        }
        let marker = data[i + 1];
        if marker == 0xC0 || marker == 0xC2 {
            // SOF0 or SOF2
            if i + 9 < data.len() {
                let h = u16::from_be_bytes([data[i + 5], data[i + 6]]) as u32;
                let w = u16::from_be_bytes([data[i + 7], data[i + 8]]) as u32;
                return Some((w, h));
            }
            return None;
        }
        // Skip to next marker
        if i + 3 < data.len() {
            let len = u16::from_be_bytes([data[i + 2], data[i + 3]]) as usize;
            i += 2 + len;
        } else {
            break;
        }
    }
    None
}

fn read_gif_dimensions(data: &[u8]) -> Option<(u32, u32)> {
    // GIF: bytes 6-7 = width, 8-9 = height (little-endian)
    if data.len() < 10 || !data.starts_with(b"GIF8") {
        return None;
    }
    let w = u16::from_le_bytes([data[6], data[7]]) as u32;
    let h = u16::from_le_bytes([data[8], data[9]]) as u32;
    Some((w, h))
}

fn read_bmp_dimensions(data: &[u8]) -> Option<(u32, u32)> {
    // BMP: bytes 18-21 = width, 22-25 = height (little-endian, signed for height)
    if data.len() < 26 || !data.starts_with(b"BM") {
        return None;
    }
    let w = u32::from_le_bytes([data[18], data[19], data[20], data[21]]);
    let h_signed = i32::from_le_bytes([data[22], data[23], data[24], data[25]]);
    let h = h_signed.unsigned_abs();
    Some((w, h))
}
