use std::path::Path;
use std::sync::Arc;

use arrow::array::{
    ArrayRef, BooleanArray, StringArray, UInt32Array, UInt64Array,
};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use parquet::arrow::ArrowWriter;
use parquet::basic::Compression;
use parquet::file::properties::WriterProperties;

use crate::model::{Chunk, ElementType};

/// Write chunks in Parquet format using Arrow/Parquet with Snappy compression.
pub fn write(chunks: &[Chunk], output_dir: &Path, input_filename: &str) -> Result<(), crate::Error> {
    let filename = format!("{}.parquet", input_filename);
    let path = output_dir.join(filename);

    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new("text", DataType::Utf8, false),
        Field::new("token_count", DataType::UInt64, false),
        Field::new("page_start", DataType::UInt32, false),
        Field::new("page_end", DataType::UInt32, false),
        Field::new("element_types", DataType::Utf8, false),
        Field::new("has_table", DataType::Boolean, false),
    ]));

    // Build column arrays from chunks
    let ids: Vec<&str> = chunks.iter().map(|c| c.id.as_str()).collect();
    let texts: Vec<&str> = chunks.iter().map(|c| c.text.as_str()).collect();
    let token_counts: Vec<u64> = chunks.iter().map(|c| c.token_count as u64).collect();
    let page_starts: Vec<u32> = chunks.iter().map(|c| c.page_start).collect();
    let page_ends: Vec<u32> = chunks.iter().map(|c| c.page_end).collect();

    let element_types_strs: Vec<String> = chunks
        .iter()
        .map(|c| {
            let mut types: Vec<String> = c
                .source_blocks
                .iter()
                .map(|b| format!("{:?}", b.element_type))
                .collect();
            types.dedup();
            types.join(",")
        })
        .collect();
    let element_types_refs: Vec<&str> = element_types_strs.iter().map(|s| s.as_str()).collect();

    let has_tables: Vec<bool> = chunks
        .iter()
        .map(|c| {
            c.source_blocks
                .iter()
                .any(|b| b.element_type == ElementType::Table)
        })
        .collect();

    let columns: Vec<ArrayRef> = vec![
        Arc::new(StringArray::from(ids)),
        Arc::new(StringArray::from(texts)),
        Arc::new(UInt64Array::from(token_counts)),
        Arc::new(UInt32Array::from(page_starts)),
        Arc::new(UInt32Array::from(page_ends)),
        Arc::new(StringArray::from(element_types_refs)),
        Arc::new(BooleanArray::from(has_tables)),
    ];

    let batch = RecordBatch::try_new(schema.clone(), columns)
        .map_err(|e| crate::Error::Serialization(format!("Failed to create RecordBatch: {}", e)))?;

    let file = std::fs::File::create(&path)
        .map_err(|e| crate::Error::Io(format!("Failed to create {}: {}", path.display(), e)))?;

    let props = WriterProperties::builder()
        .set_compression(Compression::SNAPPY)
        .build();

    let mut writer = ArrowWriter::try_new(file, schema, Some(props))
        .map_err(|e| crate::Error::Serialization(format!("Failed to create Parquet writer: {}", e)))?;

    writer
        .write(&batch)
        .map_err(|e| crate::Error::Serialization(format!("Failed to write Parquet batch: {}", e)))?;

    writer
        .close()
        .map_err(|e| crate::Error::Serialization(format!("Failed to close Parquet writer: {}", e)))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Block, Chunk, ElementType};
    use tempfile::TempDir;

    #[test]
    fn test_write_parquet() {
        let dir = TempDir::new().unwrap();
        let chunks = vec![
            Chunk {
                id: "chunk-0".to_string(),
                text: "Hello world".to_string(),
                token_count: 3,
                source_blocks: vec![Block::new(ElementType::NarrativeText, "Hello world")],
                page_start: 1,
                page_end: 1,
                overlap_prefix: None,
                confidence: 1.0,
            },
            Chunk {
                id: "chunk-1".to_string(),
                text: "Table data".to_string(),
                token_count: 2,
                source_blocks: vec![Block::new(ElementType::Table, "Table data")],
                page_start: 2,
                page_end: 2,
                overlap_prefix: None,
                confidence: 0.95,
            },
        ];
        write(&chunks, dir.path(), "test").unwrap();

        let path = dir.path().join("test.parquet");
        assert!(path.exists());
        // Verify it is a valid Parquet file by checking file size is non-trivial
        let meta = std::fs::metadata(&path).unwrap();
        assert!(meta.len() > 100, "Parquet file should have non-trivial size");
    }

    #[test]
    fn test_write_parquet_empty() {
        let dir = TempDir::new().unwrap();
        let chunks: Vec<Chunk> = vec![];
        write(&chunks, dir.path(), "empty").unwrap();
        let path = dir.path().join("empty.parquet");
        assert!(path.exists());
    }
}
