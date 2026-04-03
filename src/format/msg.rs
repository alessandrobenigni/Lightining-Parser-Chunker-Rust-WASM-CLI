use std::io::Read;
use std::path::Path;

use crate::model::{Block, ElementType};

pub struct MsgParser;

impl super::FormatParser for MsgParser {
    fn parse(&self, path: &Path) -> Result<Vec<Block>, crate::Error> {
        let file =
            std::fs::File::open(path).map_err(|e| crate::Error::Io(e.to_string()))?;
        let mut comp = cfb::CompoundFile::open(file)
            .map_err(|e| crate::Error::Parse(format!("Failed to open MSG compound file: {e}")))?;

        let mut blocks = Vec::new();

        // Subject
        if let Some(subject) = read_string_prop(&mut comp, "0037") {
            if !subject.is_empty() {
                blocks.push(Block {
                    element_type: ElementType::Title,
                    text: subject,
                    page: 1,
                    confidence: 0.85,
                    ..Block::default()
                });
            }
        }

        // Sender Name
        if let Some(from) = read_string_prop(&mut comp, "0C1A") {
            if !from.is_empty() {
                blocks.push(Block {
                    element_type: ElementType::EmailHeader,
                    text: format!("From: {from}"),
                    page: 1,
                    confidence: 0.85,
                    ..Block::default()
                });
            }
        }

        // Display To
        if let Some(to) = read_string_prop(&mut comp, "0E04") {
            if !to.is_empty() {
                blocks.push(Block {
                    element_type: ElementType::EmailHeader,
                    text: format!("To: {to}"),
                    page: 1,
                    confidence: 0.85,
                    ..Block::default()
                });
            }
        }

        // Body
        if let Some(body) = read_string_prop(&mut comp, "1000") {
            let trimmed = body.trim().to_string();
            if !trimmed.is_empty() {
                blocks.push(Block {
                    element_type: ElementType::EmailBody,
                    text: trimmed,
                    page: 1,
                    confidence: 0.85,
                    ..Block::default()
                });
            }
        }

        if blocks.is_empty() {
            blocks.push(Block {
                element_type: ElementType::NarrativeText,
                text: "Empty email".into(),
                page: 1,
                confidence: 1.0,
                ..Block::default()
            });
        }

        Ok(blocks)
    }

    fn supported_extensions(&self) -> &[&str] {
        &["msg"]
    }
}

/// Read a string property from the MSG compound file.
///
/// MSG files store properties in streams named `__substg1.0_PPPPTTTT` where
/// PPPP is the property ID and TTTT is the type tag:
/// - `001F` = Unicode (UTF-16LE)
/// - `001E` = ANSI (Windows-1252 / Latin-1)
fn read_string_prop<F: Read + std::io::Seek>(
    comp: &mut cfb::CompoundFile<F>,
    prop_id: &str,
) -> Option<String> {
    // Try Unicode first (001F), then ANSI (001E)
    let unicode_path = format!("/__substg1.0_{prop_id}001F");
    if let Some(text) = read_stream_as_utf16le(comp, &unicode_path) {
        return Some(text);
    }

    let ansi_path = format!("/__substg1.0_{prop_id}001E");
    read_stream_as_ansi(comp, &ansi_path)
}

fn read_stream_as_utf16le<F: Read + std::io::Seek>(
    comp: &mut cfb::CompoundFile<F>,
    path: &str,
) -> Option<String> {
    let mut stream = comp.open_stream(path).ok()?;
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).ok()?;
    if buf.is_empty() {
        return None;
    }
    // Decode UTF-16LE: each code unit is 2 bytes
    let u16s: Vec<u16> = buf
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    let text = String::from_utf16_lossy(&u16s);
    // Strip trailing null characters
    let trimmed = text.trim_end_matches('\0').to_string();
    if trimmed.is_empty() { None } else { Some(trimmed) }
}

fn read_stream_as_ansi<F: Read + std::io::Seek>(
    comp: &mut cfb::CompoundFile<F>,
    path: &str,
) -> Option<String> {
    let mut stream = comp.open_stream(path).ok()?;
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).ok()?;
    if buf.is_empty() {
        return None;
    }
    // Decode as Windows-1252 (superset of Latin-1)
    let (text, _, _) = encoding_rs::WINDOWS_1252.decode(&buf);
    let trimmed = text.trim_end_matches('\0').to_string();
    if trimmed.is_empty() { None } else { Some(trimmed) }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal MSG (OLE2/CFB) file in memory with the given properties,
    /// write it to a temp file, and parse it.
    fn build_msg_and_parse(
        subject: Option<&str>,
        from: Option<&str>,
        to: Option<&str>,
        body: Option<&str>,
    ) -> Vec<Block> {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.msg");

        // Create a CFB compound file and write UTF-16LE streams
        {
            let file = std::fs::File::create(&path).unwrap();
            let mut comp = cfb::CompoundFile::create(file).unwrap();

            if let Some(s) = subject {
                write_utf16le_stream(&mut comp, "/__substg1.0_0037001F", s);
            }
            if let Some(s) = from {
                write_utf16le_stream(&mut comp, "/__substg1.0_0C1A001F", s);
            }
            if let Some(s) = to {
                write_utf16le_stream(&mut comp, "/__substg1.0_0E04001F", s);
            }
            if let Some(s) = body {
                write_utf16le_stream(&mut comp, "/__substg1.0_1000001F", s);
            }
        }

        let parser = MsgParser;
        <MsgParser as crate::format::FormatParser>::parse(&parser, &path).unwrap()
    }

    fn write_utf16le_stream<F: std::io::Read + std::io::Write + std::io::Seek>(
        comp: &mut cfb::CompoundFile<F>,
        stream_path: &str,
        value: &str,
    ) {
        use std::io::Write;
        let mut stream = comp.create_stream(stream_path).unwrap();
        let encoded: Vec<u8> = value
            .encode_utf16()
            .flat_map(|u| u.to_le_bytes())
            .collect();
        stream.write_all(&encoded).unwrap();
    }

    #[test]
    fn parse_full_msg() {
        let blocks = build_msg_and_parse(
            Some("Test Subject"),
            Some("Alice"),
            Some("Bob"),
            Some("Hello, world!"),
        );

        assert_eq!(blocks.len(), 4);
        assert_eq!(blocks[0].element_type, ElementType::Title);
        assert_eq!(blocks[0].text, "Test Subject");
        assert_eq!(blocks[1].element_type, ElementType::EmailHeader);
        assert_eq!(blocks[1].text, "From: Alice");
        assert_eq!(blocks[2].element_type, ElementType::EmailHeader);
        assert_eq!(blocks[2].text, "To: Bob");
        assert_eq!(blocks[3].element_type, ElementType::EmailBody);
        assert_eq!(blocks[3].text, "Hello, world!");

        for b in &blocks {
            assert!((b.confidence - 0.85).abs() < f32::EPSILON);
        }
    }

    #[test]
    fn parse_empty_msg() {
        let blocks = build_msg_and_parse(None, None, None, None);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].text, "Empty email");
    }

    #[test]
    fn parse_subject_only() {
        let blocks = build_msg_and_parse(Some("Only Subject"), None, None, None);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].element_type, ElementType::Title);
        assert_eq!(blocks[0].text, "Only Subject");
    }

    #[test]
    fn supported_extensions() {
        let parser = MsgParser;
        assert_eq!(
            <MsgParser as crate::format::FormatParser>::supported_extensions(&parser),
            &["msg"]
        );
    }
}
