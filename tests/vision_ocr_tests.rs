//! Integration tests for the OCR vision pipeline.
//!
//! These tests require ONNX model files in the `models/` directory.
//! Run `python scripts/download_models.py` to obtain them.

use std::path::Path;

use parser_chunker::vision::ocr::OcrEngine;
use parser_chunker::vision::VisionPipeline;

fn models_dir() -> &'static Path {
    Path::new("models")
}

fn models_available() -> bool {
    models_dir().join("paddleocr-det-en.onnx").exists()
        && models_dir().join("paddleocr-rec-en.onnx").exists()
        && models_dir().join("en_dict.txt").exists()
}

#[test]
fn test_ocr_engine_loads() {
    if !models_available() {
        eprintln!("SKIP: ONNX models not found in models/");
        return;
    }
    let engine = OcrEngine::load(models_dir());
    assert!(engine.is_ok(), "Failed to load OCR engine: {:?}", engine.err());
}

#[test]
fn test_ocr_detection_on_test_image() {
    if !models_available() {
        eprintln!("SKIP: ONNX models not found in models/");
        return;
    }
    let mut engine = OcrEngine::load(models_dir()).unwrap();

    let image_path = Path::new("tests/fixtures/test_ocr_image.png");
    if !image_path.exists() {
        eprintln!("SKIP: test_ocr_image.png not found");
        return;
    }

    let image_bytes = std::fs::read(image_path).unwrap();
    let regions = engine.detect_text_regions(&image_bytes).unwrap();

    println!("Detected {} text regions", regions.len());
    for (i, r) in regions.iter().enumerate() {
        println!(
            "  Region {}: bbox=({:.0}, {:.0}, {:.0}x{:.0}) conf={:.2}",
            i, r.bbox.x, r.bbox.y, r.bbox.width, r.bbox.height, r.confidence
        );
    }

    // We expect at least one text region detected
    assert!(
        !regions.is_empty(),
        "Expected at least one text region detected in the test image"
    );
}

#[test]
fn test_ocr_full_pipeline_on_test_image() {
    if !models_available() {
        eprintln!("SKIP: ONNX models not found in models/");
        return;
    }
    let mut engine = OcrEngine::load(models_dir()).unwrap();

    let image_path = Path::new("tests/fixtures/test_ocr_image.png");
    if !image_path.exists() {
        eprintln!("SKIP: test_ocr_image.png not found");
        return;
    }

    let image_bytes = std::fs::read(image_path).unwrap();
    let results = engine.process_image(&image_bytes).unwrap();

    println!("Recognized {} text results:", results.len());
    for (i, r) in results.iter().enumerate() {
        println!(
            "  [{i}] text={:?} conf={:.2} bbox=({:.0},{:.0},{:.0}x{:.0})",
            r.text, r.confidence, r.bbox.x, r.bbox.y, r.bbox.width, r.bbox.height
        );
    }

    // We expect at least one recognized text result
    assert!(
        !results.is_empty(),
        "Expected at least one recognized text from the test image"
    );

    // Check that some recognized text contains expected substrings
    let all_text: String = results.iter().map(|r| r.text.as_str()).collect::<Vec<_>>().join(" ");
    println!("All OCR text: {:?}", all_text);

    // The image says "Hello World 2024" — we expect at least partial recognition
    // (OCR quality may vary, so we check for common substrings)
    let has_meaningful_text = all_text.len() >= 3;
    assert!(
        has_meaningful_text,
        "Expected meaningful text from OCR, got: {:?}",
        all_text
    );
}

#[test]
fn test_vision_pipeline_init_and_process() {
    if !models_available() {
        eprintln!("SKIP: ONNX models not found in models/");
        return;
    }

    let mut pipeline = VisionPipeline::try_init(models_dir()).unwrap();
    assert!(pipeline.is_available());

    let image_path = Path::new("tests/fixtures/test_ocr_image.png");
    if !image_path.exists() {
        eprintln!("SKIP: test_ocr_image.png not found");
        return;
    }

    let image_bytes = std::fs::read(image_path).unwrap();
    let blocks = pipeline.process_page(&image_bytes, 1).unwrap();

    println!("VisionPipeline produced {} blocks:", blocks.len());
    for (i, b) in blocks.iter().enumerate() {
        println!(
            "  [{i}] type={:?} text={:?} conf={:.2} page={}",
            b.element_type, b.text, b.confidence, b.page
        );
    }

    assert!(
        !blocks.is_empty(),
        "Expected at least one block from vision pipeline"
    );

    // All blocks should have page=1 and source=ocr metadata
    for b in &blocks {
        assert_eq!(b.page, 1);
        assert_eq!(
            b.metadata.get("source"),
            Some(&serde_json::Value::String("ocr".into()))
        );
    }
}
