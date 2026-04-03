use std::path::Path;

use crate::model::{Block, ElementType};
use crate::model::block::TableData;

pub struct CsvTsvParser;

impl super::FormatParser for CsvTsvParser {
    fn parse(&self, path: &Path) -> Result<Vec<Block>, crate::Error> {
        let content = std::fs::read(path).map_err(|e| crate::Error::Io(e.to_string()))?;
        let text = String::from_utf8_lossy(&content);

        // Detect delimiter
        let delimiter = detect_delimiter(&text);

        let mut rdr = csv::ReaderBuilder::new()
            .delimiter(delimiter)
            .has_headers(true)
            .flexible(true)
            .from_reader(text.as_bytes());

        let headers: Option<Vec<String>> = rdr
            .headers()
            .ok()
            .map(|h| h.iter().map(|s| s.to_string()).collect());

        // Pre-allocate rows with estimated count (heuristic: ~1 row per 50 bytes)
        let estimated_rows = (text.len() / 50).max(16);
        let mut rows: Vec<Vec<String>> = Vec::with_capacity(estimated_rows);
        for result in rdr.records() {
            match result {
                Ok(record) => {
                    rows.push(record.iter().map(|s| s.to_string()).collect());
                }
                Err(_) => continue,
            }
        }

        let table_text = if let Some(ref hdrs) = headers {
            format!(
                "Table with {} columns ({}) and {} rows",
                hdrs.len(),
                hdrs.join(", "),
                rows.len()
            )
        } else {
            format!("Table with {} rows", rows.len())
        };

        Ok(vec![Block {
            element_type: ElementType::Table,
            text: table_text,
            page: 1,
            confidence: 1.0,
            table_data: Some(TableData { rows, headers }),
            ..Block::default()
        }])
    }

    fn supported_extensions(&self) -> &[&str] {
        &["csv", "tsv", "tab"]
    }
}

fn detect_delimiter(text: &str) -> u8 {
    let first_lines: String = text.lines().take(5).collect::<Vec<_>>().join("\n");
    let tab_count = first_lines.matches('\t').count();
    let comma_count = first_lines.matches(',').count();
    let semicolon_count = first_lines.matches(';').count();
    let pipe_count = first_lines.matches('|').count();

    let max = tab_count.max(comma_count).max(semicolon_count).max(pipe_count);
    if max == 0 {
        return b',';
    }
    if max == tab_count { b'\t' }
    else if max == semicolon_count { b';' }
    else if max == pipe_count { b'|' }
    else { b',' }
}
