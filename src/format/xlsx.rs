use std::path::Path;

use calamine::{open_workbook_auto, Data, Reader};

use crate::model::{Block, ElementType};
use crate::model::block::TableData;

pub struct XlsxParser;

impl super::FormatParser for XlsxParser {
    fn parse(&self, path: &Path) -> Result<Vec<Block>, crate::Error> {
        let mut workbook =
            open_workbook_auto(path).map_err(|e| crate::Error::Parse(e.to_string()))?;

        let sheet_names: Vec<String> = workbook.sheet_names().to_vec();
        let mut blocks = Vec::new();

        for (sheet_idx, sheet_name) in sheet_names.iter().enumerate() {
            let range = workbook
                .worksheet_range(sheet_name)
                .map_err(|e| crate::Error::Parse(e.to_string()))?;

            // Sheet title block
            blocks.push(Block {
                element_type: ElementType::Title,
                text: sheet_name.clone(),
                page: (sheet_idx as u32) + 1,
                confidence: 0.95,
                ..Block::default()
            });

            let mut rows: Vec<Vec<String>> = Vec::new();
            for row in range.rows() {
                let cells: Vec<String> = row
                    .iter()
                    .map(|cell| match cell {
                        Data::Empty => String::new(),
                        Data::String(s) => s.clone(),
                        Data::Float(f) => {
                            // Display integers without decimal point
                            if f.fract() == 0.0 && f.abs() < i64::MAX as f64 {
                                format!("{}", *f as i64)
                            } else {
                                format!("{f}")
                            }
                        }
                        Data::Int(i) => format!("{i}"),
                        Data::Bool(b) => format!("{b}"),
                        Data::Error(e) => format!("#ERR:{e:?}"),
                        Data::DateTime(dt) => format!("{dt}"),
                        Data::DateTimeIso(s) => s.clone(),
                        Data::DurationIso(s) => s.clone(),
                    })
                    .collect();
                rows.push(cells);
            }

            if rows.is_empty() {
                continue;
            }

            // Treat the first row as headers
            let headers = rows.remove(0);
            let num_cols = headers.len();
            let num_rows = rows.len();

            let text = format!(
                "Sheet \"{sheet_name}\": {num_cols} columns ({}) and {num_rows} data rows",
                headers.join(", ")
            );

            blocks.push(Block {
                element_type: ElementType::Table,
                text,
                page: (sheet_idx as u32) + 1,
                confidence: 0.95,
                table_data: Some(TableData {
                    rows,
                    headers: Some(headers),
                }),
                ..Block::default()
            });
        }

        if blocks.is_empty() {
            blocks.push(Block {
                element_type: ElementType::NarrativeText,
                text: "Empty spreadsheet".into(),
                page: 1,
                confidence: 0.95,
                ..Block::default()
            });
        }

        Ok(blocks)
    }

    fn supported_extensions(&self) -> &[&str] {
        &["xlsx", "xls", "xlsb", "ods"]
    }
}
