use std::fs;
use std::path::Path;

use serde::Serialize;

use crate::model::{Chunk, ElementType};

/// An element in the Unstructured.io compatible output schema.
#[derive(Debug, Serialize)]
struct UnstructuredElement {
    #[serde(rename = "type")]
    element_type: &'static str,
    text: String,
    metadata: UnstructuredMetadata,
}

#[derive(Debug, Serialize)]
struct UnstructuredMetadata {
    page_number: u32,
    filename: String,
}

/// Map Parser Chunker ElementType to Unstructured.io taxonomy string.
fn map_element_type(et: &ElementType) -> &'static str {
    match et {
        ElementType::Title => "Title",
        ElementType::NarrativeText => "NarrativeText",
        ElementType::ListItem => "ListItem",
        ElementType::Table => "Table",
        ElementType::Image => "Image",
        ElementType::Header => "Header",
        ElementType::Footer => "Footer",
        ElementType::CodeBlock => "UncategorizedText",
        ElementType::EmailBody => "NarrativeText",
        ElementType::EmailHeader => "Header",
        // Everything else maps to UncategorizedText
        ElementType::PageBreak => "UncategorizedText",
        ElementType::Caption => "NarrativeText",
        ElementType::Formula => "UncategorizedText",
        ElementType::Address => "NarrativeText",
        ElementType::Unknown => "UncategorizedText",
    }
}

/// Write chunks in Unstructured-compatible JSON format.
///
/// This is element-level output: one JSON object per source block (not per chunk).
/// The output is a flat JSON array matching Unstructured.io's element schema.
pub fn write_unstructured(
    chunks: &[Chunk],
    output_dir: &Path,
    input_filename: &str,
) -> Result<(), crate::Error> {
    let filename = format!("{}.json", input_filename);
    let path = output_dir.join(filename);

    let mut elements: Vec<UnstructuredElement> = Vec::new();

    for chunk in chunks {
        if chunk.source_blocks.is_empty() {
            // No source blocks: emit the chunk text as NarrativeText
            elements.push(UnstructuredElement {
                element_type: "NarrativeText",
                text: chunk.text.clone(),
                metadata: UnstructuredMetadata {
                    page_number: chunk.page_start,
                    filename: input_filename.to_string(),
                },
            });
        } else {
            for block in &chunk.source_blocks {
                if block.text.is_empty() {
                    continue;
                }
                elements.push(UnstructuredElement {
                    element_type: map_element_type(&block.element_type),
                    text: block.text.clone(),
                    metadata: UnstructuredMetadata {
                        page_number: block.page,
                        filename: input_filename.to_string(),
                    },
                });
            }
        }
    }

    let json = serde_json::to_string_pretty(&elements)
        .map_err(|e| crate::Error::Serialization(format!("Compat JSON serialization failed: {}", e)))?;

    fs::write(&path, json)
        .map_err(|e| crate::Error::Io(format!("Failed to write {}: {}", path.display(), e)))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Block, Chunk, ElementType};
    use tempfile::TempDir;

    #[test]
    fn test_write_unstructured_basic() {
        let dir = TempDir::new().unwrap();
        let chunks = vec![Chunk {
            id: "chunk-0".to_string(),
            text: "Hello world".to_string(),
            token_count: 3,
            source_blocks: vec![
                Block::new(ElementType::Title, "My Title"),
                Block::new(ElementType::NarrativeText, "Hello world"),
            ],
            page_start: 1,
            page_end: 1,
            overlap_prefix: None,
            confidence: 1.0,
        }];
        write_unstructured(&chunks, dir.path(), "test").unwrap();

        let content = fs::read_to_string(dir.path().join("test.json")).unwrap();
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0]["type"], "Title");
        assert_eq!(parsed[1]["type"], "NarrativeText");
        assert_eq!(parsed[0]["metadata"]["filename"], "test");
    }

    #[test]
    fn test_element_type_mapping() {
        assert_eq!(map_element_type(&ElementType::Title), "Title");
        assert_eq!(map_element_type(&ElementType::NarrativeText), "NarrativeText");
        assert_eq!(map_element_type(&ElementType::ListItem), "ListItem");
        assert_eq!(map_element_type(&ElementType::Table), "Table");
        assert_eq!(map_element_type(&ElementType::Image), "Image");
        assert_eq!(map_element_type(&ElementType::Header), "Header");
        assert_eq!(map_element_type(&ElementType::Footer), "Footer");
        assert_eq!(map_element_type(&ElementType::CodeBlock), "UncategorizedText");
        assert_eq!(map_element_type(&ElementType::EmailBody), "NarrativeText");
        assert_eq!(map_element_type(&ElementType::EmailHeader), "Header");
    }
}
