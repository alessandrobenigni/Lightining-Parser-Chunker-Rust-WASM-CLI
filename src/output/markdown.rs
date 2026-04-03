use std::fmt::Write as FmtWrite;
use std::fs;
use std::path::Path;

use crate::model::{Chunk, ElementType};

/// Write chunks as human-readable markdown. Headings render with correct levels,
/// tables render as GFM markdown tables, and chunk boundaries are clearly marked.
pub fn write(chunks: &[Chunk], output_dir: &Path, input_filename: &str) -> Result<(), crate::Error> {
    let filename = format!("{}.md", input_filename);
    let path = output_dir.join(filename);

    let mut output = String::new();
    writeln!(output, "# Chunked Output: {}\n", input_filename)
        .map_err(|e| crate::Error::Serialization(e.to_string()))?;

    for (i, chunk) in chunks.iter().enumerate() {
        // Chunk boundary marker
        writeln!(output, "---\n")
            .map_err(|e| crate::Error::Serialization(e.to_string()))?;
        writeln!(
            output,
            "**Chunk {} / {}** | tokens: {} | pages: {}-{}\n",
            i + 1,
            chunks.len(),
            chunk.token_count,
            chunk.page_start,
            chunk.page_end,
        )
        .map_err(|e| crate::Error::Serialization(e.to_string()))?;

        if let Some(ref prefix) = chunk.overlap_prefix {
            if !prefix.is_empty() {
                writeln!(output, "> *Overlap:* {}\n", prefix)
                    .map_err(|e| crate::Error::Serialization(e.to_string()))?;
            }
        }

        // Render source blocks with appropriate formatting
        if chunk.source_blocks.is_empty() {
            // No source block metadata — render raw text
            writeln!(output, "{}\n", chunk.text)
                .map_err(|e| crate::Error::Serialization(e.to_string()))?;
        } else {
            for block in &chunk.source_blocks {
                match block.element_type {
                    ElementType::Title => {
                        // Determine heading level from hierarchy depth (default to h2)
                        let level = if block.hierarchy.is_empty() { 2 } else { block.hierarchy.len().min(6) };
                        let hashes = "#".repeat(level);
                        writeln!(output, "{} {}\n", hashes, block.text)
                            .map_err(|e| crate::Error::Serialization(e.to_string()))?;
                    }
                    ElementType::Header => {
                        let level = if block.hierarchy.is_empty() { 3 } else { block.hierarchy.len().min(6) };
                        let hashes = "#".repeat(level);
                        writeln!(output, "{} {}\n", hashes, block.text)
                            .map_err(|e| crate::Error::Serialization(e.to_string()))?;
                    }
                    ElementType::Table => {
                        render_table_markdown(&mut output, block)?;
                        writeln!(output).map_err(|e| crate::Error::Serialization(e.to_string()))?;
                    }
                    ElementType::CodeBlock => {
                        writeln!(output, "```\n{}\n```\n", block.text)
                            .map_err(|e| crate::Error::Serialization(e.to_string()))?;
                    }
                    ElementType::ListItem => {
                        writeln!(output, "- {}", block.text)
                            .map_err(|e| crate::Error::Serialization(e.to_string()))?;
                    }
                    ElementType::Formula => {
                        writeln!(output, "$${}$$\n", block.text)
                            .map_err(|e| crate::Error::Serialization(e.to_string()))?;
                    }
                    ElementType::Caption => {
                        writeln!(output, "*{}*\n", block.text)
                            .map_err(|e| crate::Error::Serialization(e.to_string()))?;
                    }
                    ElementType::Image => {
                        writeln!(output, "![image]({})\n", block.text)
                            .map_err(|e| crate::Error::Serialization(e.to_string()))?;
                    }
                    _ => {
                        writeln!(output, "{}\n", block.text)
                            .map_err(|e| crate::Error::Serialization(e.to_string()))?;
                    }
                }
            }
        }
    }

    fs::write(&path, output)
        .map_err(|e| crate::Error::Io(format!("Failed to write {}: {}", path.display(), e)))?;

    Ok(())
}

/// Render a table block as a GFM markdown table.
fn render_table_markdown(output: &mut String, block: &crate::model::Block) -> Result<(), crate::Error> {
    if let Some(ref table_data) = block.table_data {
        // Render headers if present
        if let Some(ref headers) = table_data.headers {
            write!(output, "| {} |", headers.join(" | "))
                .map_err(|e| crate::Error::Serialization(e.to_string()))?;
            writeln!(output).map_err(|e| crate::Error::Serialization(e.to_string()))?;
            write!(output, "|{}|", headers.iter().map(|_| " --- ").collect::<Vec<_>>().join("|"))
                .map_err(|e| crate::Error::Serialization(e.to_string()))?;
            writeln!(output).map_err(|e| crate::Error::Serialization(e.to_string()))?;
        }

        // Render rows
        for row in &table_data.rows {
            write!(output, "| {} |", row.join(" | "))
                .map_err(|e| crate::Error::Serialization(e.to_string()))?;
            writeln!(output).map_err(|e| crate::Error::Serialization(e.to_string()))?;
        }
    } else {
        // No structured table data — render raw text
        writeln!(output, "{}", block.text)
            .map_err(|e| crate::Error::Serialization(e.to_string()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Block, Chunk, ElementType, TableData};
    use tempfile::TempDir;

    #[test]
    fn test_write_markdown_basic() {
        let dir = TempDir::new().unwrap();
        let chunks = vec![Chunk {
            id: "chunk-0".to_string(),
            text: "Hello world".to_string(),
            token_count: 3,
            source_blocks: vec![Block::new(ElementType::NarrativeText, "Hello world")],
            page_start: 1,
            page_end: 1,
            overlap_prefix: None,
            confidence: 1.0,
        }];
        write(&chunks, dir.path(), "test").unwrap();

        let content = fs::read_to_string(dir.path().join("test.md")).unwrap();
        assert!(content.contains("Hello world"));
        assert!(content.contains("Chunk 1 / 1"));
    }

    #[test]
    fn test_write_markdown_with_table() {
        let dir = TempDir::new().unwrap();
        let mut block = Block::new(ElementType::Table, "A|B\n1|2");
        block.table_data = Some(TableData {
            rows: vec![vec!["1".to_string(), "2".to_string()]],
            headers: Some(vec!["A".to_string(), "B".to_string()]),
        });

        let chunks = vec![Chunk {
            id: "chunk-0".to_string(),
            text: "A|B\n1|2".to_string(),
            token_count: 5,
            source_blocks: vec![block],
            page_start: 1,
            page_end: 1,
            overlap_prefix: None,
            confidence: 1.0,
        }];
        write(&chunks, dir.path(), "table_test").unwrap();

        let content = fs::read_to_string(dir.path().join("table_test.md")).unwrap();
        assert!(content.contains("| A | B |"));
        assert!(content.contains("| 1 | 2 |"));
    }

    #[test]
    fn test_write_markdown_with_heading() {
        let dir = TempDir::new().unwrap();
        let chunks = vec![Chunk {
            id: "chunk-0".to_string(),
            text: "My Title".to_string(),
            token_count: 3,
            source_blocks: vec![Block::new(ElementType::Title, "My Title")],
            page_start: 1,
            page_end: 1,
            overlap_prefix: None,
            confidence: 1.0,
        }];
        write(&chunks, dir.path(), "heading_test").unwrap();

        let content = fs::read_to_string(dir.path().join("heading_test.md")).unwrap();
        assert!(content.contains("## My Title"));
    }
}
