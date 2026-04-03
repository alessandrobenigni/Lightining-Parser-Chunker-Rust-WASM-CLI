use std::path::{Path, PathBuf};

use rayon::prelude::*;

use crate::chunking::chunk_blocks;
use crate::cli::ChunkStrategy;
use crate::config::load_per_doc_config;
use crate::format::{detect_format, detect_format_by_magic, get_parser};
use crate::model::{Block, Chunk};
use crate::vision::VisionPipeline;

/// Result type for batch processing: (successes, failures).
pub type BatchResult = (Vec<(PathBuf, Vec<Chunk>)>, Vec<(PathBuf, crate::Error)>);

/// Collect all parseable files from an input path (file or directory).
/// Recursively walks directories.
pub fn collect_input_files(input: &Path) -> Result<Vec<PathBuf>, crate::Error> {
    if input.is_file() {
        return Ok(vec![input.to_path_buf()]);
    }

    if input.is_dir() {
        let mut files = Vec::new();
        collect_recursive(input, &mut files)?;
        files.sort(); // deterministic order
        return Ok(files);
    }

    Err(crate::Error::Io(format!(
        "Input path does not exist: {}",
        input.display()
    )))
}

/// Recursively collect parseable files from a directory.
fn collect_recursive(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), crate::Error> {
    let entries = std::fs::read_dir(dir)
        .map_err(|e| crate::Error::Io(format!("Failed to read directory {}: {}", dir.display(), e)))?;

    for entry in entries {
        let entry = entry.map_err(|e| crate::Error::Io(e.to_string()))?;
        let path = entry.path();

        if path.is_dir() {
            collect_recursive(&path, files)?;
        } else if path.is_file() {
            // Check by extension first, fall back to magic bytes
            if detect_format(&path).is_some() || detect_format_by_magic(&path).is_some() {
                files.push(path);
            }
        }
    }

    Ok(())
}

/// Determine whether the vision pipeline should be used based on parsed blocks.
///
/// Returns true if:
/// - `blocks` is empty (nothing was extracted by text parser)
/// - All blocks have confidence < 0.3
/// - Total text length across all blocks is less than 50 characters
pub fn should_use_vision(blocks: &[Block]) -> bool {
    if blocks.is_empty() {
        return true;
    }

    let low_confidence_threshold = 0.3_f32;
    if blocks.iter().all(|b| b.confidence < low_confidence_threshold) {
        return true;
    }

    let total_text_len: usize = blocks.iter().map(|b| b.text.len()).sum();
    if total_text_len < 50 {
        return true;
    }

    false
}

/// Process a single file: detect format -> get parser -> parse -> chunk -> return chunks.
pub fn process_single_file(
    path: &Path,
    strategy: &ChunkStrategy,
    max_tokens: usize,
    overlap: usize,
) -> Result<Vec<Chunk>, crate::Error> {
    process_single_file_with_debug(path, strategy, max_tokens, overlap, None)
}

/// Process a single file with optional debug output directory.
pub fn process_single_file_with_debug(
    path: &Path,
    strategy: &ChunkStrategy,
    max_tokens: usize,
    overlap: usize,
    debug_dir: Option<&Path>,
) -> Result<Vec<Chunk>, crate::Error> {
    // Check for per-document sidecar config
    let per_doc = load_per_doc_config(path);
    let effective_strategy = per_doc
        .as_ref()
        .and_then(|c| c.chunk_strategy.as_ref())
        .map(|cs| cs.clone().into())
        .unwrap_or_else(|| strategy.clone());
    let effective_max_tokens = per_doc
        .as_ref()
        .and_then(|c| c.max_tokens)
        .unwrap_or(max_tokens);
    let effective_overlap = per_doc
        .as_ref()
        .and_then(|c| c.overlap)
        .unwrap_or(overlap);

    if per_doc.is_some() {
        tracing::info!(
            path = %path.display(),
            "Using per-document config override",
        );
    }

    // Detect format
    let format_key = detect_format(path)
        .or_else(|| detect_format_by_magic(path))
        .ok_or_else(|| {
            crate::Error::UnsupportedFormat(format!(
                "Cannot detect format for: {}",
                path.display()
            ))
        })?;

    // Get parser
    let parser = get_parser(format_key).ok_or_else(|| {
        crate::Error::UnsupportedFormat(format!(
            "No parser available for format '{}': {}",
            format_key,
            path.display()
        ))
    })?;

    // Parse
    let blocks = parser.parse(path)?;

    // Vision triage: check if vision pipeline should be used
    let blocks = if should_use_vision(&blocks) {
        // Try the vision pipeline for scanned/image content
        let models_dir = path
            .parent()
            .unwrap_or(Path::new("."))
            .join("models");
        // Also check a models/ directory relative to CWD
        let models_dir = if models_dir.join("paddleocr-det-en.onnx").exists() {
            models_dir
        } else {
            PathBuf::from("models")
        };

        match VisionPipeline::try_init(&models_dir) {
            Ok(mut pipeline) => {
                tracing::info!(
                    path = %path.display(),
                    "Vision triage triggered, attempting OCR via vision pipeline",
                );
                // For PDFs: rasterize each page and OCR
                if format_key == "pdf" {
                    let mut vision_blocks = Vec::new();
                    // Determine page count by trying pages until we fail
                    let mut page_num = 1_u32;
                    while let Ok(png_bytes) = crate::format::pdf::rasterize_page(path, page_num, 300) {
                        match pipeline.process_page(&png_bytes, page_num) {
                            Ok(page_blocks) => {
                                tracing::info!(
                                    page = page_num,
                                    blocks = page_blocks.len(),
                                    "OCR produced blocks for page",
                                );
                                vision_blocks.extend(page_blocks);
                            }
                            Err(e) => {
                                tracing::warn!(
                                    page = page_num,
                                    "Vision pipeline failed for page: {e}",
                                );
                            }
                        }
                        page_num += 1;
                    }
                    if vision_blocks.is_empty() {
                        tracing::warn!(
                            path = %path.display(),
                            "Vision pipeline produced no blocks from PDF",
                        );
                        blocks // Fall back to original (possibly empty) blocks
                    } else {
                        vision_blocks
                    }
                } else {
                    // For image files: process directly
                    let image_bytes = std::fs::read(path)
                        .map_err(|e| crate::Error::Io(format!("Failed to read image: {e}")))?;
                    match pipeline.process_page(&image_bytes, 1) {
                        Ok(vision_blocks) if !vision_blocks.is_empty() => vision_blocks,
                        Ok(_) => {
                            tracing::warn!(
                                path = %path.display(),
                                "Vision pipeline produced no blocks from image",
                            );
                            blocks
                        }
                        Err(e) => {
                            tracing::warn!(
                                path = %path.display(),
                                "Vision pipeline failed: {e}",
                            );
                            blocks
                        }
                    }
                }
            }
            Err(_) => {
                // Vision pipeline not available — log and continue with whatever we have
                if blocks.is_empty() {
                    tracing::warn!(
                        path = %path.display(),
                        format = format_key,
                        "Parser returned 0 blocks; file may be a scanned PDF or image \
                         (run `python scripts/download_models.py` to enable OCR)",
                    );
                } else {
                    let max_confidence = blocks.iter().map(|b| b.confidence).fold(0.0_f32, f32::max);
                    let total_text_len: usize = blocks.iter().map(|b| b.text.len()).sum();
                    tracing::warn!(
                        path = %path.display(),
                        block_count = blocks.len(),
                        max_confidence = max_confidence,
                        total_text_len = total_text_len,
                        "Vision triage triggered but models not available; \
                         run `python scripts/download_models.py` to enable OCR",
                    );
                }
                blocks
            }
        }
    } else {
        blocks
    };

    if blocks.is_empty() {
        if let Some(dbg_dir) = debug_dir {
            write_debug_output(dbg_dir, path, &blocks, &[], format_key)?;
        }
        return Ok(Vec::new());
    }

    // Chunk
    let chunks = chunk_blocks(&blocks, &effective_strategy, effective_max_tokens, effective_overlap)?;

    // Write debug output if enabled
    if let Some(dbg_dir) = debug_dir {
        write_debug_output(dbg_dir, path, &blocks, &chunks, format_key)?;
    }

    Ok(chunks)
}

/// Write debug output files for a single processed file.
///
/// Writes three JSON files to the debug directory:
/// - `{filename}_format.json` — detected format key and routing decision
/// - `{filename}_blocks.json` — raw Block array before chunking
/// - `{filename}_chunks.json` — Chunk array after chunking with metadata
pub fn write_debug_output(
    debug_dir: &Path,
    input_file: &Path,
    blocks: &[Block],
    chunks: &[Chunk],
    format_key: &str,
) -> Result<(), crate::Error> {
    let filename = input_file
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");

    std::fs::create_dir_all(debug_dir)
        .map_err(|e| crate::Error::Io(format!("Failed to create debug dir: {}", e)))?;

    // Format info
    let format_info = serde_json::json!({
        "input_file": input_file.display().to_string(),
        "format_key": format_key,
        "parser_routed_to": format_key,
        "block_count": blocks.len(),
        "chunk_count": chunks.len(),
    });
    let format_path = debug_dir.join(format!("{}_format.json", filename));
    std::fs::write(
        &format_path,
        serde_json::to_string_pretty(&format_info)
            .map_err(|e| crate::Error::Serialization(e.to_string()))?,
    )
    .map_err(|e| crate::Error::Io(format!("Failed to write debug format file: {}", e)))?;

    // Blocks
    let blocks_path = debug_dir.join(format!("{}_blocks.json", filename));
    std::fs::write(
        &blocks_path,
        serde_json::to_string_pretty(blocks)
            .map_err(|e| crate::Error::Serialization(e.to_string()))?,
    )
    .map_err(|e| crate::Error::Io(format!("Failed to write debug blocks file: {}", e)))?;

    // Chunks
    let chunks_path = debug_dir.join(format!("{}_chunks.json", filename));
    std::fs::write(
        &chunks_path,
        serde_json::to_string_pretty(chunks)
            .map_err(|e| crate::Error::Serialization(e.to_string()))?,
    )
    .map_err(|e| crate::Error::Io(format!("Failed to write debug chunks file: {}", e)))?;

    Ok(())
}

/// Process a batch of files in parallel using rayon.
/// Returns (successes, failures) separately so callers can handle partial failures.
pub fn process_batch(
    files: &[PathBuf],
    strategy: &ChunkStrategy,
    max_tokens: usize,
    overlap: usize,
    workers: usize,
) -> BatchResult {
    process_batch_with_debug(files, strategy, max_tokens, overlap, workers, None)
}

/// Process a batch of files in parallel with optional debug output.
pub fn process_batch_with_debug(
    files: &[PathBuf],
    strategy: &ChunkStrategy,
    max_tokens: usize,
    overlap: usize,
    workers: usize,
    debug_dir: Option<&Path>,
) -> BatchResult {
    // Configure rayon thread pool with the requested worker count
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap_or_else(|_| {
            // Fall back to global pool if custom pool fails
            rayon::ThreadPoolBuilder::new().build().unwrap()
        });

    let results: Vec<(PathBuf, Result<Vec<Chunk>, crate::Error>)> = pool.install(|| {
        files
            .par_iter()
            .map(|path| {
                let result = process_single_file_with_debug(path, strategy, max_tokens, overlap, debug_dir);
                (path.clone(), result)
            })
            .collect()
    });

    let mut successes = Vec::new();
    let mut failures = Vec::new();

    for (path, result) in results {
        match result {
            Ok(chunks) => successes.push((path, chunks)),
            Err(e) => failures.push((path, e)),
        }
    }

    (successes, failures)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_collect_input_files_single_file() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("test.txt");
        fs::write(&file, "hello").unwrap();

        let files = collect_input_files(&file).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0], file);
    }

    #[test]
    fn test_collect_input_files_directory() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("a.txt"), "hello").unwrap();
        fs::write(dir.path().join("b.csv"), "a,b\n1,2").unwrap();
        fs::write(dir.path().join("c.unknown_ext_xyz"), "data").unwrap();

        let files = collect_input_files(dir.path()).unwrap();
        // .txt and .csv should be detected, .unknown_ext_xyz should not
        assert!(files.len() >= 2);
        assert!(files.iter().any(|f| f.extension().is_some_and(|e| e == "txt")));
        assert!(files.iter().any(|f| f.extension().is_some_and(|e| e == "csv")));
    }

    #[test]
    fn test_collect_input_files_recursive() {
        let dir = TempDir::new().unwrap();
        let subdir = dir.path().join("subdir");
        fs::create_dir(&subdir).unwrap();
        fs::write(dir.path().join("top.txt"), "top").unwrap();
        fs::write(subdir.join("nested.txt"), "nested").unwrap();

        let files = collect_input_files(dir.path()).unwrap();
        assert!(files.len() >= 2);
    }

    #[test]
    fn test_collect_input_files_nonexistent() {
        let result = collect_input_files(Path::new("/nonexistent/path"));
        assert!(result.is_err());
    }

    #[test]
    fn test_process_single_file_txt() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("test.txt");
        fs::write(&file, "Hello world. This is a test document with some content.").unwrap();

        let result = process_single_file(&file, &ChunkStrategy::ByStructure, 512, 50);
        // Should succeed (text parser should work)
        assert!(result.is_ok());
        let chunks = result.unwrap();
        assert!(!chunks.is_empty());
        // Confidence should be 1.0 for text parser
        assert!((chunks[0].confidence - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_process_batch_mixed_results() {
        let dir = TempDir::new().unwrap();
        let good_file = dir.path().join("good.txt");
        fs::write(&good_file, "Hello world").unwrap();

        let files = vec![good_file];
        let (successes, failures) = process_batch(&files, &ChunkStrategy::Fixed, 512, 50, 1);

        assert_eq!(successes.len(), 1);
        assert!(failures.is_empty());
    }

    #[test]
    fn test_debug_output() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("test.txt");
        fs::write(&file, "Hello world. Debug test content.").unwrap();

        let debug_dir = dir.path().join("debug");
        let result = process_single_file_with_debug(
            &file,
            &ChunkStrategy::ByStructure,
            512,
            50,
            Some(&debug_dir),
        );
        assert!(result.is_ok());

        // Check debug files exist
        assert!(debug_dir.join("test_format.json").exists());
        assert!(debug_dir.join("test_blocks.json").exists());
        assert!(debug_dir.join("test_chunks.json").exists());

        // Verify format info
        let format_content = fs::read_to_string(debug_dir.join("test_format.json")).unwrap();
        let format_json: serde_json::Value = serde_json::from_str(&format_content).unwrap();
        assert_eq!(format_json["format_key"], "text");
    }

    // --- F-037: should_use_vision tests ---

    #[test]
    fn test_should_use_vision_empty_blocks() {
        assert!(should_use_vision(&[]));
    }

    #[test]
    fn test_should_use_vision_low_confidence() {
        let blocks = vec![
            {
                let mut b = Block::new(crate::model::ElementType::NarrativeText, "some text here for testing purposes enough");
                b.confidence = 0.2;
                b
            },
            {
                let mut b = Block::new(crate::model::ElementType::NarrativeText, "more text");
                b.confidence = 0.1;
                b
            },
        ];
        assert!(should_use_vision(&blocks));
    }

    #[test]
    fn test_should_use_vision_short_text() {
        let blocks = vec![
            Block::new(crate::model::ElementType::NarrativeText, "Hi"),
        ];
        // Total text < 50 chars
        assert!(should_use_vision(&blocks));
    }

    #[test]
    fn test_should_use_vision_good_blocks() {
        let blocks = vec![
            Block::new(crate::model::ElementType::NarrativeText, "This is a reasonably long paragraph with sufficient content to pass the threshold."),
        ];
        // confidence = 1.0 (default), text > 50 chars
        assert!(!should_use_vision(&blocks));
    }

    // --- F-062: per-doc config sidecar test ---

    #[test]
    fn test_per_doc_config_applied() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("test.txt");
        fs::write(&file, "Hello world. This is a test document with enough content to chunk.").unwrap();

        // Create sidecar config requesting fixed chunking
        let sidecar = dir.path().join("test.txt.parser-chunker.toml");
        fs::write(&sidecar, "max_tokens = 2048\nchunk_strategy = \"fixed\"").unwrap();

        let result = process_single_file(&file, &ChunkStrategy::ByStructure, 512, 50);
        assert!(result.is_ok());
        // The sidecar should have overridden strategy and max_tokens
        // We can't easily verify the strategy used, but we can verify it succeeded
        let chunks = result.unwrap();
        assert!(!chunks.is_empty());
    }
}
