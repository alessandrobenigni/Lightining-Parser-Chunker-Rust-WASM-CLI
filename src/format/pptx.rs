use std::io::Read;
use std::path::Path;

use quick_xml::events::Event;
use quick_xml::reader::Reader;

use crate::model::{Block, ElementType};

pub struct PptxParser;

impl super::FormatParser for PptxParser {
    fn parse(&self, path: &Path) -> Result<Vec<Block>, crate::Error> {
        let file =
            std::fs::File::open(path).map_err(|e| crate::Error::Io(e.to_string()))?;
        let mut archive =
            zip::ZipArchive::new(file).map_err(|e| crate::Error::Parse(e.to_string()))?;

        // Collect slide entry names and sort by slide number
        let mut slide_entries: Vec<String> = Vec::new();
        for i in 0..archive.len() {
            if let Ok(entry) = archive.by_index(i) {
                let name = entry.name().to_string();
                if is_slide_file(&name) {
                    slide_entries.push(name);
                }
            }
        }
        slide_entries.sort_by_key(|name| extract_slide_number(name));

        let mut blocks = Vec::new();

        for (slide_idx, slide_name) in slide_entries.iter().enumerate() {
            let mut xml_content = String::new();
            {
                let mut entry = archive
                    .by_name(slide_name)
                    .map_err(|e| crate::Error::Parse(e.to_string()))?;
                entry
                    .read_to_string(&mut xml_content)
                    .map_err(|e| crate::Error::Io(e.to_string()))?;
            }

            let slide_num = (slide_idx as u32) + 1;
            let paragraphs = extract_paragraphs(&xml_content)?;

            let mut first = true;
            for para in &paragraphs {
                if para.is_empty() {
                    continue;
                }
                if first {
                    blocks.push(Block {
                        element_type: ElementType::Title,
                        text: para.clone(),
                        page: slide_num,
                        confidence: 0.85,
                        ..Block::default()
                    });
                    first = false;
                } else {
                    blocks.push(Block {
                        element_type: ElementType::NarrativeText,
                        text: para.clone(),
                        page: slide_num,
                        confidence: 0.85,
                        ..Block::default()
                    });
                }
            }
        }

        if blocks.is_empty() {
            blocks.push(Block {
                element_type: ElementType::NarrativeText,
                text: "Empty presentation".into(),
                page: 1,
                confidence: 1.0,
                ..Block::default()
            });
        }

        Ok(blocks)
    }

    fn supported_extensions(&self) -> &[&str] {
        &["pptx"]
    }
}

/// Check if a ZIP entry is a slide XML file (e.g. ppt/slides/slide1.xml)
fn is_slide_file(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.starts_with("ppt/slides/slide") && lower.ends_with(".xml") && !lower.contains("layout")
}

/// Extract the slide number from a filename like "ppt/slides/slide12.xml"
fn extract_slide_number(name: &str) -> u32 {
    let lower = name.to_lowercase();
    // Find digits between "slide" and ".xml"
    if let Some(start) = lower.find("slide") {
        let after = &lower[start + 5..];
        let digits: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
        digits.parse().unwrap_or(0)
    } else {
        0
    }
}

/// Extract paragraphs from PPTX slide XML.
/// Each <a:p> produces one paragraph; text runs (<a:t>) within the same
/// paragraph are joined with a space (spike-006 finding).
fn extract_paragraphs(xml: &str) -> Result<Vec<String>, crate::Error> {
    let mut reader = Reader::from_str(xml);
    let mut paragraphs: Vec<String> = Vec::new();
    let mut current_para = String::new();
    let mut in_a_t = false;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let local = e.local_name();
                let name = std::str::from_utf8(local.as_ref()).unwrap_or("");
                if name == "p"
                    && e.name()
                        .as_ref()
                        .starts_with(b"a:")
                {
                    // Starting a new <a:p>
                    current_para.clear();
                }
                if name == "t"
                    && e.name()
                        .as_ref()
                        .starts_with(b"a:")
                {
                    in_a_t = true;
                }
            }
            Ok(Event::Text(ref e)) => {
                if in_a_t {
                    let text = e.unescape().unwrap_or_default().to_string();
                    if !current_para.is_empty() && !text.is_empty() {
                        current_para.push(' ');
                    }
                    current_para.push_str(&text);
                }
            }
            Ok(Event::End(ref e)) => {
                let local = e.local_name();
                let name = std::str::from_utf8(local.as_ref()).unwrap_or("");
                if name == "t" && e.name().as_ref().starts_with(b"a:") {
                    in_a_t = false;
                }
                if name == "p" && e.name().as_ref().starts_with(b"a:") {
                    let trimmed = current_para.trim().to_string();
                    if !trimmed.is_empty() {
                        paragraphs.push(trimmed);
                    }
                    current_para.clear();
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(crate::Error::Parse(format!("PPTX XML error: {e}"))),
            _ => {}
        }
        buf.clear();
    }

    Ok(paragraphs)
}
