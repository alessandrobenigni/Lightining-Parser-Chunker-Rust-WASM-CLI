use std::io::Read;
use std::path::Path;

use quick_xml::events::Event;
use quick_xml::reader::Reader;

use crate::model::{Block, ElementType};
use crate::model::block::TableData;

pub struct DocxParser;

impl super::FormatParser for DocxParser {
    fn parse(&self, path: &Path) -> Result<Vec<Block>, crate::Error> {
        let file = std::fs::File::open(path).map_err(|e| crate::Error::Io(e.to_string()))?;
        let mut archive = zip::ZipArchive::new(file).map_err(|e| crate::Error::Parse(e.to_string()))?;

        // Read document.xml from the DOCX archive
        let mut doc_xml = String::new();
        {
            let mut doc_file = archive
                .by_name("word/document.xml")
                .map_err(|e| crate::Error::Parse(format!("Cannot find word/document.xml: {}", e)))?;
            doc_file.read_to_string(&mut doc_xml).map_err(|e| crate::Error::Io(e.to_string()))?;
        }

        parse_docx_xml(&doc_xml)
    }

    fn supported_extensions(&self) -> &[&str] {
        &["docx", "doc"]
    }
}

#[allow(unused_assignments, unused_variables)]
fn parse_docx_xml(xml: &str) -> Result<Vec<Block>, crate::Error> {
    let mut reader = Reader::from_str(xml);
    let mut blocks = Vec::new();
    let mut current_text = String::new();
    let mut in_paragraph = false;
    let mut in_run = false;
    let mut in_text = false;
    let mut in_table = false;
    let mut in_table_row = false;
    let mut in_table_cell = false;
    let mut current_style: Option<String> = None;
    let mut table_rows: Vec<Vec<String>> = Vec::new();
    let mut current_row: Vec<String> = Vec::new();
    let mut cell_text = String::new();

    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                let local_name = e.local_name();
                let name = std::str::from_utf8(local_name.as_ref()).unwrap_or("");

                match name {
                    "p" => {
                        in_paragraph = true;
                        current_text.clear();
                        current_style = None;
                    }
                    "r" => { in_run = true; }
                    "t" => { in_text = true; }
                    "pStyle" => {
                        // Extract style val attribute
                        for attr in e.attributes().flatten() {
                            if attr.key.local_name().as_ref() == b"val" {
                                current_style = Some(String::from_utf8_lossy(&attr.value).to_string());
                            }
                        }
                    }
                    "tbl" => {
                        in_table = true;
                        table_rows.clear();
                    }
                    "tr" => {
                        in_table_row = true;
                        current_row.clear();
                    }
                    "tc" => {
                        in_table_cell = true;
                        cell_text.clear();
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(ref e)) => {
                if in_text {
                    let text = e.unescape().unwrap_or_default().to_string();
                    if in_table_cell {
                        cell_text.push_str(&text);
                    } else {
                        current_text.push_str(&text);
                    }
                }
            }
            Ok(Event::End(ref e)) => {
                let local_name = e.local_name();
                let name = std::str::from_utf8(local_name.as_ref()).unwrap_or("");

                match name {
                    "t" => { in_text = false; }
                    "r" => { in_run = false; }
                    "p" => {
                        in_paragraph = false;
                        let text = current_text.trim().to_string();
                        if !text.is_empty() && !in_table_cell {
                            let element_type = match current_style.as_deref() {
                                Some(s) if s.contains("Heading") || s.contains("heading") || s.contains("Title") => {
                                    ElementType::Title
                                }
                                Some(s) if s.contains("ListParagraph") || s.contains("List") => {
                                    ElementType::ListItem
                                }
                                _ => ElementType::NarrativeText,
                            };

                            let mut block = Block {
                                element_type,
                                text,
                                page: 1,
                                confidence: 0.9,
                                ..Block::default()
                            };

                            if let Some(ref style) = current_style {
                                if style.contains("Heading") || style.contains("heading") {
                                    // Extract heading level
                                    let level: String = style.chars().filter(|c| c.is_ascii_digit()).collect();
                                    if !level.is_empty() {
                                        block.hierarchy = vec![format!("h{}", level)];
                                    }
                                }
                            }

                            blocks.push(block);
                        }
                        if in_table_cell {
                            cell_text.push_str(current_text.trim());
                        }
                        current_text.clear();
                    }
                    "tc" => {
                        in_table_cell = false;
                        current_row.push(cell_text.trim().to_string());
                    }
                    "tr" => {
                        in_table_row = false;
                        if !current_row.is_empty() {
                            table_rows.push(std::mem::take(&mut current_row));
                        }
                    }
                    "tbl" => {
                        in_table = false;
                        if !table_rows.is_empty() {
                            let headers = if !table_rows.is_empty() {
                                Some(table_rows.remove(0))
                            } else {
                                None
                            };
                            let text = format!(
                                "Table: {} columns, {} rows",
                                headers.as_ref().map_or(0, |h| h.len()),
                                table_rows.len()
                            );
                            blocks.push(Block {
                                element_type: ElementType::Table,
                                text,
                                page: 1,
                                confidence: 0.9,
                                table_data: Some(TableData {
                                    rows: std::mem::take(&mut table_rows),
                                    headers,
                                }),
                                ..Block::default()
                            });
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(crate::Error::Parse(format!("XML parse error: {}", e))),
            _ => {}
        }
        buf.clear();
    }

    Ok(blocks)
}
