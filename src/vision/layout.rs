use serde::{Deserialize, Serialize};

use crate::model::BoundingBox;

/// Types of regions detected by the layout model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegionType {
    Title,
    Text,
    Table,
    Image,
    List,
    Header,
    Footer,
}

/// A region detected by the layout analysis model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedRegion {
    /// Bounding box of the detected region on the page.
    pub bbox: BoundingBox,
    /// Classified region type.
    pub region_type: RegionType,
    /// Model confidence score (0.0 to 1.0).
    pub confidence: f32,
}

/// Layout detection engine using an ONNX model (e.g., Heron/YOLO).
///
/// Detects document regions (titles, text blocks, tables, images, etc.) from page images,
/// returning bounding boxes with classification and confidence scores.
pub struct LayoutDetector {
    // Will hold: ort::Session when model is loaded
    _private: (),
}

impl LayoutDetector {
    /// Create a new LayoutDetector. Will require an ONNX session in the future.
    pub fn new() -> Self {
        Self { _private: () }
    }

    /// Detect regions in a page image.
    ///
    /// The image should be raw pixel data (e.g., RGBA). Preprocessing to the model's
    /// expected input format (resize, normalize, CHW tensor) will be done internally.
    ///
    /// Returns detected regions sorted by vertical position (top to bottom).
    pub fn detect_regions(&self, _image: &[u8]) -> Result<Vec<DetectedRegion>, crate::Error> {
        // TODO: When ONNX model is available:
        // 1. Preprocess image (resize to model input, normalize, convert to CHW tensor)
        // 2. Run inference via ort::Session
        // 3. Post-process outputs (NMS, threshold filtering)
        // 4. Map class indices to RegionType
        // 5. Sort by y-coordinate (reading order)
        Err(crate::Error::NotImplemented(
            "Layout detection model not loaded",
        ))
    }

    /// Filter detected regions by confidence threshold.
    pub fn filter_by_confidence(
        regions: &[DetectedRegion],
        threshold: f32,
    ) -> Vec<DetectedRegion> {
        regions
            .iter()
            .filter(|r| r.confidence >= threshold)
            .cloned()
            .collect()
    }

    /// Sort regions into reading order (top-to-bottom, left-to-right).
    pub fn sort_reading_order(regions: &mut [DetectedRegion]) {
        regions.sort_by(|a, b| {
            let y_cmp = a.bbox.y.partial_cmp(&b.bbox.y).unwrap_or(std::cmp::Ordering::Equal);
            if y_cmp == std::cmp::Ordering::Equal {
                a.bbox.x.partial_cmp(&b.bbox.x).unwrap_or(std::cmp::Ordering::Equal)
            } else {
                y_cmp
            }
        });
    }
}

impl Default for LayoutDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_regions_not_implemented() {
        let detector = LayoutDetector::new();
        let result = detector.detect_regions(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_filter_by_confidence() {
        let regions = vec![
            DetectedRegion {
                bbox: BoundingBox { x: 0.0, y: 0.0, width: 100.0, height: 50.0 },
                region_type: RegionType::Text,
                confidence: 0.9,
            },
            DetectedRegion {
                bbox: BoundingBox { x: 0.0, y: 50.0, width: 100.0, height: 50.0 },
                region_type: RegionType::Table,
                confidence: 0.3,
            },
        ];

        let filtered = LayoutDetector::filter_by_confidence(&regions, 0.5);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].region_type, RegionType::Text);
    }

    #[test]
    fn test_sort_reading_order() {
        let mut regions = vec![
            DetectedRegion {
                bbox: BoundingBox { x: 200.0, y: 100.0, width: 50.0, height: 50.0 },
                region_type: RegionType::Text,
                confidence: 0.9,
            },
            DetectedRegion {
                bbox: BoundingBox { x: 10.0, y: 0.0, width: 50.0, height: 50.0 },
                region_type: RegionType::Title,
                confidence: 0.95,
            },
            DetectedRegion {
                bbox: BoundingBox { x: 10.0, y: 100.0, width: 50.0, height: 50.0 },
                region_type: RegionType::Text,
                confidence: 0.8,
            },
        ];

        LayoutDetector::sort_reading_order(&mut regions);
        assert_eq!(regions[0].region_type, RegionType::Title);
        // Same y=100.0 row: x=10 before x=200
        assert!((regions[1].bbox.x - 10.0).abs() < f32::EPSILON);
        assert!((regions[2].bbox.x - 200.0).abs() < f32::EPSILON);
    }
}
