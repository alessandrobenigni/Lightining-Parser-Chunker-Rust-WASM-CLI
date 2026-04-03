pub mod layout;
pub mod ocr;
pub mod table;

use std::path::Path;

use crate::model::{Block, BoundingBox, ElementType};

use self::ocr::OcrEngine;

/// The vision pipeline processes page images through layout detection, table extraction, and OCR.
/// It is activated when the heuristic text parser produces insufficient results (scanned PDFs, image files).
pub struct VisionPipeline {
    /// OCR engine (PaddleOCR detection + recognition)
    ocr_engine: Option<OcrEngine>,
    /// Whether the pipeline was successfully initialized
    initialized: bool,
}

impl VisionPipeline {
    pub fn new() -> Self {
        Self {
            ocr_engine: None,
            initialized: false,
        }
    }

    /// Try to initialize the vision pipeline by loading ONNX models from the given directory.
    ///
    /// Currently loads:
    /// - PaddleOCR detection + recognition models for OCR
    ///
    /// Future additions:
    /// - Layout detection model (Heron/YOLO)
    /// - TableFormer ONNX model
    pub fn try_init(models_dir: &Path) -> Result<Self, crate::Error> {
        // Check for OCR models (the minimum needed for vision)
        let det_model = models_dir.join("paddleocr-det-en.onnx");
        let rec_model = models_dir.join("paddleocr-rec-en.onnx");

        if !det_model.exists() || !rec_model.exists() {
            return Err(crate::Error::NotImplemented(
                "Vision pipeline models not found. Run `python scripts/download_models.py` to download ONNX models.",
            ));
        }

        tracing::info!("Initializing vision pipeline from {}", models_dir.display());

        let ocr_engine = OcrEngine::load(models_dir)?;

        Ok(Self {
            ocr_engine: Some(ocr_engine),
            initialized: true,
        })
    }

    pub fn is_available(&self) -> bool {
        self.initialized
    }

    /// Process a page image through the vision pipeline.
    ///
    /// Takes PNG image bytes and page number, runs OCR, and returns recognized text as Blocks.
    pub fn process_page(
        &mut self,
        image_data: &[u8],
        page_num: u32,
    ) -> Result<Vec<Block>, crate::Error> {
        if !self.initialized {
            return Err(crate::Error::NotImplemented(
                "Vision pipeline not initialized",
            ));
        }

        let ocr = self.ocr_engine.as_mut().ok_or(crate::Error::NotImplemented(
            "OCR engine not loaded",
        ))?;

        // Run OCR on the page image
        let recognized = ocr.process_image(image_data)?;

        if recognized.is_empty() {
            tracing::debug!("Vision pipeline: no text recognized on page {page_num}");
            return Ok(Vec::new());
        }

        tracing::debug!(
            "Vision pipeline: recognized {} text regions on page {page_num}",
            recognized.len()
        );

        // Convert recognized text regions to Blocks
        let mut blocks = Vec::new();
        for rec in &recognized {
            let mut block = Block::new(ElementType::NarrativeText, &rec.text);
            block.page = page_num;
            block.confidence = rec.confidence;
            block.bbox = Some(BoundingBox {
                x: rec.bbox.x,
                y: rec.bbox.y,
                width: rec.bbox.width,
                height: rec.bbox.height,
            });
            block
                .metadata
                .insert("source".into(), serde_json::Value::String("ocr".into()));
            blocks.push(block);
        }

        // Try to merge adjacent text regions that are on the same line into paragraphs
        let merged = merge_adjacent_blocks(blocks);

        Ok(merged)
    }
}

impl Default for VisionPipeline {
    fn default() -> Self {
        Self::new()
    }
}

/// Merge OCR blocks that are on the same horizontal line (close y-coordinates)
/// into single blocks, preserving reading order.
fn merge_adjacent_blocks(blocks: Vec<Block>) -> Vec<Block> {
    if blocks.len() <= 1 {
        return blocks;
    }

    let mut merged: Vec<Block> = Vec::new();

    for block in blocks {
        let should_merge = if let Some(last) = merged.last() {
            // Merge if the y-positions overlap significantly (same line)
            match (&last.bbox, &block.bbox) {
                (Some(last_bbox), Some(cur_bbox)) => {
                    let last_y_center = last_bbox.y + last_bbox.height / 2.0;
                    let cur_y_center = cur_bbox.y + cur_bbox.height / 2.0;
                    let max_height = last_bbox.height.max(cur_bbox.height);
                    let y_diff = (last_y_center - cur_y_center).abs();

                    // Same line: y-centers within half the max height
                    // AND horizontal gap is small (within 2x average char width estimate)
                    let gap = cur_bbox.x - (last_bbox.x + last_bbox.width);
                    y_diff < max_height * 0.5 && gap < max_height * 2.0 && gap > -max_height
                }
                _ => false,
            }
        } else {
            false
        };

        if should_merge {
            let last = merged.last_mut().unwrap();
            last.text.push(' ');
            last.text.push_str(&block.text);
            // Expand bounding box
            if let (Some(last_bbox), Some(cur_bbox)) = (&mut last.bbox, &block.bbox) {
                let new_x = last_bbox.x.min(cur_bbox.x);
                let new_y = last_bbox.y.min(cur_bbox.y);
                let new_right = (last_bbox.x + last_bbox.width).max(cur_bbox.x + cur_bbox.width);
                let new_bottom =
                    (last_bbox.y + last_bbox.height).max(cur_bbox.y + cur_bbox.height);
                last_bbox.x = new_x;
                last_bbox.y = new_y;
                last_bbox.width = new_right - new_x;
                last_bbox.height = new_bottom - new_y;
            }
            // Average confidence
            last.confidence = (last.confidence + block.confidence) / 2.0;
        } else {
            merged.push(block);
        }
    }

    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_new_pipeline_not_initialized() {
        let pipeline = VisionPipeline::new();
        assert!(!pipeline.is_available());
    }

    #[test]
    fn test_try_init_missing_models() {
        let dir = TempDir::new().unwrap();
        let result = VisionPipeline::try_init(dir.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_process_page_not_initialized() {
        let mut pipeline = VisionPipeline::new();
        let result = pipeline.process_page(&[], 1);
        assert!(result.is_err());
    }

    #[test]
    fn test_merge_adjacent_blocks_empty() {
        let result = merge_adjacent_blocks(Vec::new());
        assert!(result.is_empty());
    }

    #[test]
    fn test_merge_adjacent_blocks_single() {
        let block = Block::new(ElementType::NarrativeText, "hello");
        let result = merge_adjacent_blocks(vec![block]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].text, "hello");
    }

    #[test]
    fn test_merge_adjacent_blocks_same_line() {
        let mut b1 = Block::new(ElementType::NarrativeText, "Hello");
        b1.bbox = Some(BoundingBox {
            x: 10.0,
            y: 100.0,
            width: 50.0,
            height: 20.0,
        });

        let mut b2 = Block::new(ElementType::NarrativeText, "World");
        b2.bbox = Some(BoundingBox {
            x: 65.0,
            y: 100.0,
            width: 50.0,
            height: 20.0,
        });

        let result = merge_adjacent_blocks(vec![b1, b2]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].text, "Hello World");
    }

    #[test]
    fn test_merge_adjacent_blocks_different_lines() {
        let mut b1 = Block::new(ElementType::NarrativeText, "Line 1");
        b1.bbox = Some(BoundingBox {
            x: 10.0,
            y: 100.0,
            width: 50.0,
            height: 20.0,
        });

        let mut b2 = Block::new(ElementType::NarrativeText, "Line 2");
        b2.bbox = Some(BoundingBox {
            x: 10.0,
            y: 200.0,
            width: 50.0,
            height: 20.0,
        });

        let result = merge_adjacent_blocks(vec![b1, b2]);
        assert_eq!(result.len(), 2);
    }
}
