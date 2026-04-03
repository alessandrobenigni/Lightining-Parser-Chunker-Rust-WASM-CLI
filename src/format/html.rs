use std::path::Path;

use scraper::{Html, Selector};

use crate::model::{Block, ElementType};
use crate::model::block::TableData;

pub struct HtmlParser;

impl super::FormatParser for HtmlParser {
    fn parse(&self, path: &Path) -> Result<Vec<Block>, crate::Error> {
        let content = std::fs::read_to_string(path).map_err(|e| crate::Error::Io(e.to_string()))?;
        let document = Html::parse_document(&content);
        let mut blocks = Vec::new();

        // Extract title
        if let Ok(sel) = Selector::parse("title") {
            for el in document.select(&sel) {
                let text = el.text().collect::<String>().trim().to_string();
                if !text.is_empty() {
                    blocks.push(Block {
                        element_type: ElementType::Title,
                        text,
                        page: 1,
                        confidence: 0.95,
                        ..Block::default()
                    });
                }
            }
        }

        // Extract headings
        for level in 1..=6 {
            let tag = format!("h{}", level);
            let sel = match Selector::parse(&tag) {
                Ok(s) => s,
                Err(_) => continue,
            };
            for el in document.select(&sel) {
                let text = el.text().collect::<String>().trim().to_string();
                if !text.is_empty() {
                    let mut block = Block {
                        element_type: ElementType::Title,
                        text,
                        page: 1,
                        confidence: 0.95,
                        ..Block::default()
                    };
                    block.hierarchy = vec![format!("h{}", level)];
                    blocks.push(block);
                }
            }
        }

        // Extract paragraphs
        if let Ok(sel) = Selector::parse("p") {
            for el in document.select(&sel) {
                let text = el.text().collect::<String>().trim().to_string();
                if !text.is_empty() {
                    blocks.push(Block {
                        element_type: ElementType::NarrativeText,
                        text,
                        page: 1,
                        confidence: 0.95,
                        ..Block::default()
                    });
                }
            }
        }

        // Extract list items
        if let Ok(sel) = Selector::parse("li") {
            for el in document.select(&sel) {
                let text = el.text().collect::<String>().trim().to_string();
                if !text.is_empty() {
                    blocks.push(Block {
                        element_type: ElementType::ListItem,
                        text,
                        page: 1,
                        confidence: 0.95,
                        ..Block::default()
                    });
                }
            }
        }

        // Extract tables
        if let Ok(table_sel) = Selector::parse("table") {
            let tr_sel = Selector::parse("tr").unwrap();
            let th_sel = Selector::parse("th").unwrap();
            let td_sel = Selector::parse("td").unwrap();

            for table_el in document.select(&table_sel) {
                let mut headers: Vec<String> = Vec::new();
                let mut rows: Vec<Vec<String>> = Vec::new();

                for (i, tr) in table_el.select(&tr_sel).enumerate() {
                    let ths: Vec<String> = tr.select(&th_sel)
                        .map(|td| td.text().collect::<String>().trim().to_string())
                        .collect();

                    if !ths.is_empty() && i == 0 {
                        headers = ths;
                        continue;
                    }

                    let tds: Vec<String> = tr.select(&td_sel)
                        .map(|td| td.text().collect::<String>().trim().to_string())
                        .collect();

                    if !tds.is_empty() {
                        rows.push(tds);
                    }
                }

                let text = format!(
                    "Table: {} columns, {} rows",
                    if headers.is_empty() { rows.first().map_or(0, |r| r.len()) } else { headers.len() },
                    rows.len()
                );

                blocks.push(Block {
                    element_type: ElementType::Table,
                    text,
                    page: 1,
                    confidence: 1.0,
                    table_data: Some(TableData {
                        rows,
                        headers: if headers.is_empty() { None } else { Some(headers) },
                    }),
                    ..Block::default()
                });
            }
        }

        // Extract code blocks
        if let Ok(sel) = Selector::parse("pre, code") {
            for el in document.select(&sel) {
                let text = el.text().collect::<String>().trim().to_string();
                if !text.is_empty() {
                    blocks.push(Block {
                        element_type: ElementType::CodeBlock,
                        text,
                        page: 1,
                        confidence: 0.95,
                        ..Block::default()
                    });
                }
            }
        }

        if blocks.is_empty() {
            // Fallback: extract all text content
            let body_sel = Selector::parse("body").unwrap_or_else(|_| Selector::parse("*").unwrap());
            for el in document.select(&body_sel) {
                let text = el.text().collect::<String>().trim().to_string();
                if !text.is_empty() {
                    blocks.push(Block {
                        element_type: ElementType::NarrativeText,
                        text,
                        page: 1,
                        confidence: 0.95,
                        ..Block::default()
                    });
                    break;
                }
            }
        }

        Ok(blocks)
    }

    fn supported_extensions(&self) -> &[&str] {
        &["html", "htm", "xhtml", "mhtml", "mht"]
    }
}
