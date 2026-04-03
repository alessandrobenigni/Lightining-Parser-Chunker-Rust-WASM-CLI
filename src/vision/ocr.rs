use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::model::BoundingBox;

/// A text region detected by the OCR detection model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextRegion {
    /// Bounding box of the detected text region.
    pub bbox: BoundingBox,
    /// Confidence score from the detection model (0.0 to 1.0).
    pub confidence: f32,
}

/// A recognized text result from the OCR recognition model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecognizedText {
    /// The recognized text string.
    pub text: String,
    /// Bounding box of the text region.
    pub bbox: BoundingBox,
    /// Recognition confidence score (0.0 to 1.0).
    pub confidence: f32,
}

/// OCR engine using PaddleOCR ONNX models (detection + recognition).
///
/// Two-stage pipeline:
/// 1. Text detection (paddleocr-det-en.onnx): locates text regions in the image
/// 2. Text recognition (paddleocr-rec-en.onnx): reads text from each detected region
pub struct OcrEngine {
    det_session: ort::session::Session,
    rec_session: ort::session::Session,
    /// Character dictionary for CTC decoding (index -> char).
    char_dict: Vec<char>,
}

impl OcrEngine {
    /// Load the OCR engine from model files in the given directory.
    ///
    /// Expects:
    /// - `{models_dir}/paddleocr-det-en.onnx` (detection model)
    /// - `{models_dir}/paddleocr-rec-en.onnx` (recognition model)
    /// - `{models_dir}/en_dict.txt` (character dictionary, one char per line)
    pub fn load(models_dir: &Path) -> Result<Self, crate::Error> {
        let det_path = models_dir.join("paddleocr-det-en.onnx");
        let rec_path = models_dir.join("paddleocr-rec-en.onnx");
        let dict_path = models_dir.join("en_dict.txt");

        if !det_path.exists() {
            return Err(crate::Error::Io(format!(
                "OCR detection model not found: {}",
                det_path.display()
            )));
        }
        if !rec_path.exists() {
            return Err(crate::Error::Io(format!(
                "OCR recognition model not found: {}",
                rec_path.display()
            )));
        }
        if !dict_path.exists() {
            return Err(crate::Error::Io(format!(
                "OCR character dictionary not found: {}",
                dict_path.display()
            )));
        }

        tracing::info!("Loading OCR detection model: {}", det_path.display());
        let det_session = ort::session::Session::builder()
            .map_err(|e| crate::Error::Parse(format!("ort session builder: {e}")))?
            .commit_from_file(&det_path)
            .map_err(|e| crate::Error::Parse(format!("ort load det model: {e}")))?;

        tracing::info!("Loading OCR recognition model: {}", rec_path.display());
        let rec_session = ort::session::Session::builder()
            .map_err(|e| crate::Error::Parse(format!("ort session builder: {e}")))?
            .commit_from_file(&rec_path)
            .map_err(|e| crate::Error::Parse(format!("ort load rec model: {e}")))?;

        // Load character dictionary
        let dict_content = std::fs::read_to_string(&dict_path)
            .map_err(|e| crate::Error::Io(format!("Failed to read dict: {e}")))?;

        // Index 0 is reserved for CTC blank, so we prepend a placeholder
        let mut char_dict: Vec<char> = vec!['\0']; // blank token at index 0
        for line in dict_content.lines() {
            if let Some(ch) = line.chars().next() {
                char_dict.push(ch);
            }
        }

        tracing::info!(
            "OCR engine loaded: det={}, rec={}, dict_size={}",
            det_path.display(),
            rec_path.display(),
            char_dict.len()
        );

        Ok(Self {
            det_session,
            rec_session,
            char_dict,
        })
    }

    /// Detect text regions in an image.
    ///
    /// Input: PNG image bytes.
    /// Output: bounding boxes of text regions.
    pub fn detect_text_regions(
        &mut self,
        image_bytes: &[u8],
    ) -> Result<Vec<TextRegion>, crate::Error> {
        let img = image::load_from_memory(image_bytes)
            .map_err(|e| crate::Error::Parse(format!("Failed to decode image: {e}")))?;
        let rgb = img.to_rgb8();
        let (orig_w, orig_h) = (rgb.width(), rgb.height());

        // PaddleOCR det expects input [1, 3, H, W] where H,W are multiples of 32
        let det_h = (orig_h as usize).div_ceil(32) * 32;
        let det_w = (orig_w as usize).div_ceil(32) * 32;

        // Resize image to det_h x det_w
        let resized = image::imageops::resize(
            &rgb,
            det_w as u32,
            det_h as u32,
            image::imageops::FilterType::Lanczos3,
        );

        // Build NCHW float32 tensor with ImageNet normalization
        let mean = [0.485_f32, 0.456, 0.406];
        let std_dev = [0.229_f32, 0.224, 0.225];
        let mut tensor_data = vec![0.0_f32; 3 * det_h * det_w];

        for y in 0..det_h {
            for x in 0..det_w {
                let pixel = resized.get_pixel(x as u32, y as u32);
                for c in 0..3 {
                    let val = pixel[c] as f32 / 255.0;
                    let normalized = (val - mean[c]) / std_dev[c];
                    tensor_data[c * det_h * det_w + y * det_w + x] = normalized;
                }
            }
        }

        // Create ort tensor using (shape, data) tuple form
        let shape = vec![1_i64, 3, det_h as i64, det_w as i64];
        let input_tensor = ort::value::Tensor::from_array((shape, tensor_data))
            .map_err(|e| crate::Error::Parse(format!("ort tensor creation: {e}")))?;

        // Run detection
        let outputs = self
            .det_session
            .run(ort::inputs![input_tensor])
            .map_err(|e| crate::Error::Parse(format!("ort det inference: {e}")))?;

        // Output is a probability map [1, 1, H, W]
        let (output_shape, prob_data) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| crate::Error::Parse(format!("ort extract det output: {e}")))?;

        let _ = output_shape; // shape info available if needed

        // Threshold the probability map and find bounding boxes
        let threshold = 0.3_f32;
        let regions = extract_boxes_from_prob_map(
            prob_data,
            det_h,
            det_w,
            orig_h as usize,
            orig_w as usize,
            threshold,
        );

        Ok(regions)
    }

    /// Recognize text from a cropped RGB image region.
    ///
    /// Input: raw RGB pixel data and dimensions.
    /// Output: recognized text string and confidence.
    pub fn recognize_text_from_crop(
        &mut self,
        crop_rgb: &[u8],
        crop_w: u32,
        crop_h: u32,
    ) -> Result<(String, f32), crate::Error> {
        // PaddleOCR rec expects input [1, 3, 48, W] where W is proportional
        let target_h = 48_u32;
        let aspect = crop_w as f32 / crop_h.max(1) as f32;
        let target_w = ((target_h as f32 * aspect) as u32).max(48);
        // Round up to multiple of 32 for stability
        let target_w = ((target_w as usize).div_ceil(32) * 32) as u32;

        // Create an image from the raw RGB data and resize
        let crop_img = image::RgbImage::from_raw(crop_w, crop_h, crop_rgb.to_vec())
            .ok_or_else(|| crate::Error::Parse("Failed to create crop image".into()))?;

        let resized = image::imageops::resize(
            &crop_img,
            target_w,
            target_h,
            image::imageops::FilterType::Lanczos3,
        );

        // Build NCHW float32 tensor, normalize to [-1, 1] range (PaddleOCR rec convention)
        let h = target_h as usize;
        let w = target_w as usize;
        let mut tensor_data = vec![0.0_f32; 3 * h * w];
        for y in 0..h {
            for x in 0..w {
                let pixel = resized.get_pixel(x as u32, y as u32);
                for c in 0..3 {
                    let val = pixel[c] as f32 / 255.0;
                    // Normalize to [-1, 1]: (x / 255 - 0.5) / 0.5
                    let normalized = (val - 0.5) / 0.5;
                    tensor_data[c * h * w + y * w + x] = normalized;
                }
            }
        }

        let shape = vec![1_i64, 3, h as i64, w as i64];
        let input_tensor = ort::value::Tensor::from_array((shape, tensor_data))
            .map_err(|e| crate::Error::Parse(format!("ort tensor creation: {e}")))?;

        let outputs = self
            .rec_session
            .run(ort::inputs![input_tensor])
            .map_err(|e| crate::Error::Parse(format!("ort rec inference: {e}")))?;

        // Output shape: [1, seq_len, num_classes] -- CTC output
        let (output_shape, output_data) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| crate::Error::Parse(format!("ort extract rec output: {e}")))?;

        if output_shape.len() != 3 {
            return Err(crate::Error::Parse(format!(
                "Unexpected rec output shape: {output_shape:?}"
            )));
        }

        let seq_len = output_shape[1] as usize;
        let num_classes = output_shape[2] as usize;

        // CTC greedy decode: for each timestep, pick argmax, collapse repeats, remove blanks
        let (text, confidence) =
            ctc_greedy_decode(output_data, seq_len, num_classes, &self.char_dict);

        Ok((text, confidence))
    }

    /// Full OCR pipeline: detect text regions, then recognize text in each.
    ///
    /// Input: PNG image bytes.
    /// Returns a list of recognized text results with positions and confidence.
    pub fn process_image(
        &mut self,
        image_bytes: &[u8],
    ) -> Result<Vec<RecognizedText>, crate::Error> {
        // Decode the full image once for cropping
        let img = image::load_from_memory(image_bytes)
            .map_err(|e| crate::Error::Parse(format!("Failed to decode image: {e}")))?;
        let rgb = img.to_rgb8();

        // Step 1: Detect text regions
        let regions = self.detect_text_regions(image_bytes)?;

        if regions.is_empty() {
            tracing::debug!("OCR detection found no text regions");
            return Ok(Vec::new());
        }

        tracing::debug!("OCR detected {} text regions", regions.len());

        // Step 2: For each region, crop and recognize
        let mut results = Vec::new();
        let (img_w, img_h) = (rgb.width(), rgb.height());

        for region in &regions {
            // Convert bbox to pixel coordinates and clamp
            let x = (region.bbox.x as u32).min(img_w.saturating_sub(1));
            let y = (region.bbox.y as u32).min(img_h.saturating_sub(1));
            let w = (region.bbox.width as u32).min(img_w - x).max(1);
            let h = (region.bbox.height as u32).min(img_h - y).max(1);

            // Crop the region
            let crop = image::imageops::crop_imm(&rgb, x, y, w, h).to_image();
            let crop_rgb = crop.as_raw();

            match self.recognize_text_from_crop(crop_rgb, w, h) {
                Ok((text, conf)) => {
                    let text = text.trim().to_string();
                    if !text.is_empty() {
                        results.push(RecognizedText {
                            text,
                            bbox: region.bbox.clone(),
                            confidence: conf,
                        });
                    }
                }
                Err(e) => {
                    tracing::warn!("OCR recognition failed for region at ({x},{y}): {e}");
                }
            }
        }

        // Sort by vertical position (reading order)
        results.sort_by(|a, b| {
            a.bbox
                .y
                .partial_cmp(&b.bbox.y)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(
                    a.bbox
                        .x
                        .partial_cmp(&b.bbox.x)
                        .unwrap_or(std::cmp::Ordering::Equal),
                )
        });

        Ok(results)
    }
}

// -- Detection post-processing -----------------------------------------------

/// Extract bounding boxes from the detection probability map.
///
/// The probability map is [1, 1, det_h, det_w]. We threshold it, find connected
/// components (simplified: find contiguous regions and group them into boxes),
/// then scale coordinates back to original image dimensions.
fn extract_boxes_from_prob_map(
    prob_data: &[f32],
    det_h: usize,
    det_w: usize,
    orig_h: usize,
    orig_w: usize,
    threshold: f32,
) -> Vec<TextRegion> {
    // Create binary mask
    let mut mask = vec![false; det_h * det_w];
    for (i, &val) in prob_data.iter().enumerate().take(det_h * det_w) {
        if val > threshold {
            mask[i] = true;
        }
    }

    // Simple connected-component labeling (4-connected) using flood fill
    let mut labels = vec![0_u32; det_h * det_w];
    let mut next_label = 1_u32;
    let mut component_bounds: Vec<(usize, usize, usize, usize)> = Vec::new(); // (min_x, min_y, max_x, max_y)

    for y in 0..det_h {
        for x in 0..det_w {
            let idx = y * det_w + x;
            if mask[idx] && labels[idx] == 0 {
                // Flood fill this component
                let mut min_x = x;
                let mut min_y = y;
                let mut max_x = x;
                let mut max_y = y;
                let mut stack = vec![(x, y)];
                labels[idx] = next_label;

                while let Some((cx, cy)) = stack.pop() {
                    min_x = min_x.min(cx);
                    min_y = min_y.min(cy);
                    max_x = max_x.max(cx);
                    max_y = max_y.max(cy);

                    // 4-connected neighbors
                    let neighbors = [
                        (cx.wrapping_sub(1), cy),
                        (cx + 1, cy),
                        (cx, cy.wrapping_sub(1)),
                        (cx, cy + 1),
                    ];
                    for (nx, ny) in neighbors {
                        if nx < det_w && ny < det_h {
                            let nidx = ny * det_w + nx;
                            if mask[nidx] && labels[nidx] == 0 {
                                labels[nidx] = next_label;
                                stack.push((nx, ny));
                            }
                        }
                    }
                }

                component_bounds.push((min_x, min_y, max_x, max_y));
                next_label += 1;
            }
        }
    }

    // Scale factors from detection input to original image
    let scale_x = orig_w as f32 / det_w as f32;
    let scale_y = orig_h as f32 / det_h as f32;

    // Filter small boxes and convert to TextRegion
    let min_box_area = 100.0_f32; // minimum area in original pixels
    let mut regions = Vec::new();

    for (min_x, min_y, max_x, max_y) in component_bounds {
        let x = min_x as f32 * scale_x;
        let y = min_y as f32 * scale_y;
        let w = (max_x - min_x + 1) as f32 * scale_x;
        let h = (max_y - min_y + 1) as f32 * scale_y;

        if w * h < min_box_area {
            continue;
        }

        // Add generous padding (5px scaled, minimum 10px)
        // Text detection tends to produce tight boxes; padding helps recognition
        let pad_x = (5.0 * scale_x).max(10.0);
        let pad_y = (5.0 * scale_y).max(10.0);
        let x = (x - pad_x).max(0.0);
        let y = (y - pad_y).max(0.0);
        let w = (w + 2.0 * pad_x).min(orig_w as f32 - x);
        let h = (h + 2.0 * pad_y).min(orig_h as f32 - y);

        regions.push(TextRegion {
            bbox: BoundingBox {
                x,
                y,
                width: w,
                height: h,
            },
            confidence: 0.8, // Detection doesn't give per-box confidence easily; use default
        });
    }

    regions
}

// -- CTC decoding ------------------------------------------------------------

/// CTC greedy decode: for each timestep pick the class with highest logit,
/// collapse consecutive duplicates, remove blanks (index 0).
/// Returns (decoded_text, average_confidence).
fn ctc_greedy_decode(
    output_data: &[f32],
    seq_len: usize,
    num_classes: usize,
    char_dict: &[char],
) -> (String, f32) {
    let mut indices = Vec::with_capacity(seq_len);
    let mut confidences = Vec::new();

    for t in 0..seq_len {
        let offset = t * num_classes;
        let end = (offset + num_classes).min(output_data.len());
        let slice = &output_data[offset..end];

        // Find argmax
        let mut best_idx = 0;
        let mut best_val = f32::NEG_INFINITY;
        for (i, &v) in slice.iter().enumerate() {
            if v > best_val {
                best_val = v;
                best_idx = i;
            }
        }

        // Use sigmoid as a simple confidence estimate from the raw logit
        let conf = 1.0 / (1.0 + (-best_val).exp());

        indices.push(best_idx);
        confidences.push(conf);
    }

    // Collapse consecutive duplicates and remove blanks
    let mut text = String::new();
    let mut total_conf = 0.0_f32;
    let mut char_count = 0;
    let mut prev_idx = usize::MAX;

    for (t, &idx) in indices.iter().enumerate() {
        if idx == prev_idx {
            continue; // skip duplicate
        }
        prev_idx = idx;

        if idx == 0 {
            continue; // skip blank
        }

        if idx < char_dict.len() {
            text.push(char_dict[idx]);
            total_conf += confidences[t];
            char_count += 1;
        }
    }

    let avg_conf = if char_count > 0 {
        total_conf / char_count as f32
    } else {
        0.0
    };

    (text, avg_conf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ctc_greedy_decode_basic() {
        let char_dict = vec!['\0', 'a', 'b', 'c']; // 0=blank, 1=a, 2=b, 3=c
        // 4 timesteps, 4 classes each
        // t0: best=1 (a), t1: best=1 (a, duplicate), t2: best=0 (blank), t3: best=2 (b)
        let output = vec![
            0.0, 5.0, 1.0, 1.0, // t0 -> 'a'
            0.0, 5.0, 1.0, 1.0, // t1 -> 'a' (dup, skip)
            5.0, 0.0, 0.0, 0.0, // t2 -> blank
            0.0, 1.0, 5.0, 1.0, // t3 -> 'b'
        ];
        let (text, _conf) = ctc_greedy_decode(&output, 4, 4, &char_dict);
        assert_eq!(text, "ab");
    }

    #[test]
    fn test_ctc_greedy_decode_empty() {
        let char_dict = vec!['\0', 'a', 'b'];
        // All blanks
        let output = vec![5.0, 0.0, 0.0, 5.0, 0.0, 0.0];
        let (text, conf) = ctc_greedy_decode(&output, 2, 3, &char_dict);
        assert_eq!(text, "");
        assert!((conf - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_extract_boxes_simple() {
        // 32x32 prob map with a bright region at (4..12, 4..12)
        let det_h = 32;
        let det_w = 32;
        let mut prob = vec![0.0_f32; det_h * det_w];
        for y in 4..12 {
            for x in 4..12 {
                prob[y * det_w + x] = 0.8;
            }
        }
        let regions = extract_boxes_from_prob_map(&prob, det_h, det_w, 640, 640, 0.3);
        assert!(!regions.is_empty());
        // Should have one region roughly at the right spot
        let r = &regions[0];
        assert!(r.bbox.x >= 0.0);
        assert!(r.bbox.y >= 0.0);
        assert!(r.bbox.width > 0.0);
        assert!(r.bbox.height > 0.0);
    }

    #[test]
    fn test_extract_boxes_nothing() {
        let prob = vec![0.0_f32; 32 * 32];
        let regions = extract_boxes_from_prob_map(&prob, 32, 32, 640, 640, 0.3);
        assert!(regions.is_empty());
    }
}
