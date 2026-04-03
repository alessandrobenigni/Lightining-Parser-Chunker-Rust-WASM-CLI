pub mod compat;
pub mod json;
pub mod markdown;
pub mod parquet;

use std::path::Path;

use crate::cli::OutputFormat;
use crate::model::Chunk;

/// Write chunks to the output directory in the chosen format.
/// The `input_filename` is used to derive the output filename (e.g., "report" -> "report.json").
pub fn write_output(
    chunks: &[Chunk],
    output_dir: &Path,
    input_filename: &str,
    format: &OutputFormat,
) -> Result<(), crate::Error> {
    // Ensure the output directory exists (no-op if already created by batch caller)
    std::fs::create_dir_all(output_dir)
        .map_err(|e| crate::Error::Io(format!("Failed to create output dir: {}", e)))?;

    match format {
        OutputFormat::Json => json::write(chunks, output_dir, input_filename),
        OutputFormat::Jsonl => json::write_jsonl(chunks, output_dir, input_filename),
        OutputFormat::Parquet => parquet::write(chunks, output_dir, input_filename),
        OutputFormat::Markdown => markdown::write(chunks, output_dir, input_filename),
    }
}
