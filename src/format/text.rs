use std::path::Path;

use crate::model::{Block, ElementType};

/// Threshold for using memory-mapped I/O (64 KB).
const MMAP_THRESHOLD: u64 = 64 * 1024;

pub struct TextParser;

/// Holds file bytes — either owned (small files) or memory-mapped (large files).
enum FileBytes {
    Owned(Vec<u8>),
    Mapped(memmap2::Mmap),
}

impl AsRef<[u8]> for FileBytes {
    fn as_ref(&self) -> &[u8] {
        match self {
            FileBytes::Owned(v) => v,
            FileBytes::Mapped(m) => m,
        }
    }
}

/// Read file bytes, using mmap for files >= MMAP_THRESHOLD (zero-copy).
fn read_file_bytes(path: &Path) -> Result<FileBytes, crate::Error> {
    let metadata = std::fs::metadata(path).map_err(|e| crate::Error::Io(e.to_string()))?;
    if metadata.len() >= MMAP_THRESHOLD {
        let file = std::fs::File::open(path).map_err(|e| crate::Error::Io(e.to_string()))?;
        // SAFETY: We only read the file and don't modify it. The file is opened read-only.
        let mmap = unsafe { memmap2::Mmap::map(&file) }
            .map_err(|e| crate::Error::Io(format!("mmap failed for {}: {}", path.display(), e)))?;
        Ok(FileBytes::Mapped(mmap))
    } else {
        let bytes = std::fs::read(path).map_err(|e| crate::Error::Io(e.to_string()))?;
        Ok(FileBytes::Owned(bytes))
    }
}

impl super::FormatParser for TextParser {
    fn parse(&self, path: &Path) -> Result<Vec<Block>, crate::Error> {
        let file_bytes = read_file_bytes(path)?;
        let bytes = file_bytes.as_ref();

        // Detect encoding and decode
        let (text, _, had_errors) = encoding_rs::UTF_8.decode(bytes);
        if had_errors {
            // Try other encodings
            let (text2, _, _) = encoding_rs::WINDOWS_1252.decode(bytes);
            return Ok(parse_plain_text(&text2, path));
        }

        // Check file type by extension (case-insensitive, no allocation)
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if ext.eq_ignore_ascii_case("md") || ext.eq_ignore_ascii_case("markdown") {
            Ok(parse_markdown(&text))
        } else if ext.eq_ignore_ascii_case("xml") || ext.eq_ignore_ascii_case("xsd")
            || ext.eq_ignore_ascii_case("xsl") || ext.eq_ignore_ascii_case("svg")
            || ext.eq_ignore_ascii_case("rss") || ext.eq_ignore_ascii_case("atom")
        {
            parse_xml(&text)
        } else {
            Ok(parse_plain_text(&text, path))
        }
    }

    fn supported_extensions(&self) -> &[&str] {
        &["txt", "text", "md", "markdown", "rst", "log", "cfg", "ini", "conf", "json", "jsonl", "yaml", "yml", "xml"]
    }
}

fn parse_plain_text(text: &str, _path: &Path) -> Vec<Block> {
    // Estimate ~1 block per 500 chars as capacity hint
    let estimated_blocks = (text.len() / 500).max(1);
    let mut blocks = Vec::with_capacity(estimated_blocks);
    let mut current_para = String::new();
    let page: u32 = 1;

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if !current_para.is_empty() {
                blocks.push(Block {
                    element_type: ElementType::NarrativeText,
                    text: std::mem::take(&mut current_para),
                    page,
                    confidence: 1.0,
                    ..Block::default()
                });
            }
        } else {
            if !current_para.is_empty() {
                current_para.push(' ');
            }
            current_para.push_str(trimmed);
        }
    }

    if !current_para.is_empty() {
        blocks.push(Block {
            element_type: ElementType::NarrativeText,
            text: current_para,
            page,
            confidence: 1.0,
            ..Block::default()
        });
    }

    if blocks.is_empty() {
        blocks.push(Block {
            element_type: ElementType::NarrativeText,
            text: String::new(),
            page,
            confidence: 1.0,
            ..Block::default()
        });
    }

    blocks
}

fn parse_markdown(text: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut current_text = String::new();
    let mut in_code_block = false;
    let mut code_block = String::new();

    for line in text.lines() {
        if line.starts_with("```") {
            if in_code_block {
                // End code block
                blocks.push(Block {
                    element_type: ElementType::CodeBlock,
                    text: std::mem::take(&mut code_block),
                    page: 1,
                    confidence: 1.0,
                    ..Block::default()
                });
                in_code_block = false;
            } else {
                // Flush current text
                if !current_text.is_empty() {
                    blocks.push(Block {
                        element_type: ElementType::NarrativeText,
                        text: std::mem::take(&mut current_text),
                        page: 1,
                        confidence: 1.0,
                        ..Block::default()
                    });
                }
                in_code_block = true;
            }
            continue;
        }

        if in_code_block {
            if !code_block.is_empty() {
                code_block.push('\n');
            }
            code_block.push_str(line);
            continue;
        }

        // Headings
        if line.starts_with('#') {
            // Flush current text
            if !current_text.is_empty() {
                blocks.push(Block {
                    element_type: ElementType::NarrativeText,
                    text: std::mem::take(&mut current_text),
                    page: 1,
                    confidence: 1.0,
                    ..Block::default()
                });
            }

            let level = line.chars().take_while(|c| *c == '#').count();
            let heading_text = line[level..].trim().to_string();
            let mut block = Block {
                element_type: ElementType::Title,
                text: heading_text.clone(),
                page: 1,
                confidence: 1.0,
                ..Block::default()
            };
            block.hierarchy = vec![format!("h{}", level)];
            blocks.push(block);
            continue;
        }

        // List items
        if line.starts_with("- ") || line.starts_with("* ") || line.starts_with("+ ") {
            if !current_text.is_empty() {
                blocks.push(Block {
                    element_type: ElementType::NarrativeText,
                    text: std::mem::take(&mut current_text),
                    page: 1,
                    confidence: 1.0,
                    ..Block::default()
                });
            }
            blocks.push(Block {
                element_type: ElementType::ListItem,
                text: line[2..].trim().to_string(),
                page: 1,
                confidence: 1.0,
                ..Block::default()
            });
            continue;
        }

        // Numbered list items (handles multi-digit: 1. 10. 100. etc.)
        if line.starts_with(|c: char| c.is_ascii_digit()) {
            let num_end = line.find(|c: char| !c.is_ascii_digit()).unwrap_or(0);
            if num_end > 0 {
                if let Some(rest) = line[num_end..].strip_prefix(". ").or_else(|| line[num_end..].strip_prefix(") ")) {
                    if !current_text.is_empty() {
                        blocks.push(Block {
                            element_type: ElementType::NarrativeText,
                            text: std::mem::take(&mut current_text),
                            page: 1,
                            confidence: 1.0,
                            ..Block::default()
                        });
                    }
                    blocks.push(Block {
                        element_type: ElementType::ListItem,
                        text: rest.trim().to_string(),
                        page: 1,
                        confidence: 1.0,
                        ..Block::default()
                    });
                    continue;
                }
            }
        }

        // Empty line = paragraph break
        if line.trim().is_empty() {
            if !current_text.is_empty() {
                blocks.push(Block {
                    element_type: ElementType::NarrativeText,
                    text: std::mem::take(&mut current_text),
                    page: 1,
                    confidence: 1.0,
                    ..Block::default()
                });
            }
        } else {
            if !current_text.is_empty() {
                current_text.push(' ');
            }
            current_text.push_str(line.trim());
        }
    }

    // Flush remaining
    if in_code_block && !code_block.is_empty() {
        blocks.push(Block {
            element_type: ElementType::CodeBlock,
            text: code_block,
            page: 1,
            confidence: 1.0,
            ..Block::default()
        });
    }
    if !current_text.is_empty() {
        blocks.push(Block {
            element_type: ElementType::NarrativeText,
            text: current_text,
            page: 1,
            confidence: 1.0,
            ..Block::default()
        });
    }

    blocks
}

/// Parse XML content, extracting text from elements while preserving hierarchy.
fn parse_xml(xml: &str) -> Result<Vec<Block>, crate::Error> {
    use quick_xml::events::Event;
    use quick_xml::reader::Reader;

    let mut reader = Reader::from_str(xml);
    let mut blocks = Vec::new();
    let mut buf = Vec::new();
    let mut element_stack: Vec<String> = Vec::new();
    let mut current_text = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                // Flush accumulated text before entering a new element
                flush_xml_text(&mut current_text, &element_stack, &mut blocks);

                let local = e.local_name();
                let name = std::str::from_utf8(local.as_ref())
                    .unwrap_or("?")
                    .to_string();
                element_stack.push(name);
            }
            Ok(Event::Text(ref e)) => {
                let text = e.unescape().unwrap_or_default().to_string();
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    if !current_text.is_empty() {
                        current_text.push(' ');
                    }
                    current_text.push_str(trimmed);
                }
            }
            Ok(Event::CData(ref e)) => {
                let text = String::from_utf8_lossy(e.as_ref()).to_string();
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    if !current_text.is_empty() {
                        current_text.push(' ');
                    }
                    current_text.push_str(trimmed);
                }
            }
            Ok(Event::End(_)) => {
                flush_xml_text(&mut current_text, &element_stack, &mut blocks);
                element_stack.pop();
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(crate::Error::Parse(format!("XML parse error: {e}"))),
            _ => {}
        }
        buf.clear();
    }

    // Flush any remaining text
    flush_xml_text(&mut current_text, &element_stack, &mut blocks);

    if blocks.is_empty() {
        blocks.push(Block {
            element_type: ElementType::NarrativeText,
            text: String::new(),
            page: 1,
            confidence: 1.0,
            ..Block::default()
        });
    }

    Ok(blocks)
}

fn flush_xml_text(current_text: &mut String, element_stack: &[String], blocks: &mut Vec<Block>) {
    if current_text.is_empty() {
        return;
    }
    let text = std::mem::take(current_text);
    let hierarchy: Vec<String> = element_stack.to_vec();
    blocks.push(Block {
        element_type: ElementType::NarrativeText,
        text,
        page: 1,
        confidence: 1.0,
        hierarchy,
        ..Block::default()
    });
}
