use std::fs;
use std::io::{BufWriter, Write};
use std::path::Path;

use crate::model::Chunk;

/// Write chunks as a JSON array to a .json file.
/// Uses `to_writer` with BufWriter to avoid intermediate String allocation.
pub fn write(chunks: &[Chunk], output_dir: &Path, input_filename: &str) -> Result<(), crate::Error> {
    let filename = format!("{}.json", input_filename);
    let path = output_dir.join(filename);

    let file = fs::File::create(&path)
        .map_err(|e| crate::Error::Io(format!("Failed to create {}: {}", path.display(), e)))?;
    let writer = BufWriter::new(file);

    serde_json::to_writer_pretty(writer, chunks)
        .map_err(|e| crate::Error::Serialization(format!("JSON serialization failed: {}", e)))?;

    Ok(())
}

/// Write one chunk per line to a .jsonl file.
pub fn write_jsonl(chunks: &[Chunk], output_dir: &Path, input_filename: &str) -> Result<(), crate::Error> {
    let filename = format!("{}.jsonl", input_filename);
    let path = output_dir.join(filename);

    let file = fs::File::create(&path)
        .map_err(|e| crate::Error::Io(format!("Failed to create {}: {}", path.display(), e)))?;
    let mut writer = BufWriter::new(file);

    for chunk in chunks {
        let line = serde_json::to_string(chunk)
            .map_err(|e| crate::Error::Serialization(format!("JSONL serialization failed: {}", e)))?;
        writeln!(writer, "{}", line)
            .map_err(|e| crate::Error::Io(format!("Failed to write line: {}", e)))?;
    }

    writer.flush()
        .map_err(|e| crate::Error::Io(format!("Failed to flush JSONL writer: {}", e)))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Chunk;
    use tempfile::TempDir;

    fn sample_chunks() -> Vec<Chunk> {
        vec![
            Chunk {
                id: "chunk-0".to_string(),
                text: "Hello world".to_string(),
                token_count: 3,
                source_blocks: vec![],
                page_start: 1,
                page_end: 1,
                overlap_prefix: None,
                confidence: 1.0,
            },
            Chunk {
                id: "chunk-1".to_string(),
                text: "Second chunk".to_string(),
                token_count: 4,
                source_blocks: vec![],
                page_start: 1,
                page_end: 2,
                overlap_prefix: Some("overlap".to_string()),
                confidence: 1.0,
            },
        ]
    }

    #[test]
    fn test_write_json() {
        let dir = TempDir::new().unwrap();
        let chunks = sample_chunks();
        write(&chunks, dir.path(), "test").unwrap();

        let content = fs::read_to_string(dir.path().join("test.json")).unwrap();
        let parsed: Vec<Chunk> = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].id, "chunk-0");
    }

    #[test]
    fn test_write_jsonl() {
        let dir = TempDir::new().unwrap();
        let chunks = sample_chunks();
        write_jsonl(&chunks, dir.path(), "test").unwrap();

        let content = fs::read_to_string(dir.path().join("test.jsonl")).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 2);

        let first: Chunk = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(first.id, "chunk-0");
    }
}
