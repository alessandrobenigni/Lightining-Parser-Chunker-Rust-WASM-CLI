use std::path::Path;

use crate::model::{Block, ElementType};

pub struct RtfParser;

impl super::FormatParser for RtfParser {
    fn parse(&self, path: &Path) -> Result<Vec<Block>, crate::Error> {
        let bytes = std::fs::read(path).map_err(|e| crate::Error::Io(e.to_string()))?;
        let text = String::from_utf8_lossy(&bytes);

        if !text.starts_with("{\\rtf") {
            return Err(crate::Error::Parse(format!(
                "Not a valid RTF file (missing {{\\rtf header): {}",
                path.display()
            )));
        }

        let extracted = extract_rtf_text(&text);

        if extracted.trim().is_empty() {
            return Ok(vec![Block {
                element_type: ElementType::NarrativeText,
                text: String::new(),
                page: 1,
                confidence: 0.8,
                ..Block::default()
            }]);
        }

        // Split into paragraphs on double-newline or \par boundaries
        let mut blocks = Vec::new();
        let mut current_para = String::new();

        for line in extracted.lines() {
            if line.trim().is_empty() {
                if !current_para.is_empty() {
                    blocks.push(Block {
                        element_type: ElementType::NarrativeText,
                        text: std::mem::take(&mut current_para),
                        page: 1,
                        confidence: 0.8,
                        ..Block::default()
                    });
                }
            } else {
                if !current_para.is_empty() {
                    current_para.push(' ');
                }
                current_para.push_str(line.trim());
            }
        }

        if !current_para.is_empty() {
            blocks.push(Block {
                element_type: ElementType::NarrativeText,
                text: current_para,
                page: 1,
                confidence: 0.8,
                ..Block::default()
            });
        }

        if blocks.is_empty() {
            blocks.push(Block {
                element_type: ElementType::NarrativeText,
                text: String::new(),
                page: 1,
                confidence: 0.8,
                ..Block::default()
            });
        }

        Ok(blocks)
    }

    fn supported_extensions(&self) -> &[&str] {
        &["rtf"]
    }
}

/// Extract plain text from RTF content using a simple state-machine approach.
///
/// Strategy:
/// - Track brace nesting depth
/// - Skip content inside `{\fonttbl ...}`, `{\colortbl ...}`, `{\stylesheet ...}`,
///   `{\info ...}`, `{\header ...}`, `{\footer ...}`, `{\*\...}` destination groups
/// - Convert `\par` and `\line` to newlines
/// - Strip all other control words (`\word` optionally followed by a number and space)
/// - Decode common escape sequences: `\\`, `\{`, `\}`, `\tab`, `\~`, `\-`
fn extract_rtf_text(rtf: &str) -> String {
    let chars: Vec<char> = rtf.chars().collect();
    let len = chars.len();
    let mut result = String::new();
    let mut i = 0;
    let mut depth = 0i32;
    // Track group-skip depths: when we enter a destination group to skip,
    // record the depth at which we entered. Skip all content until depth drops below.
    let mut skip_until_depth: Option<i32> = None;

    while i < len {
        let ch = chars[i];

        // Check if we are currently skipping a destination group
        if let Some(skip_depth) = skip_until_depth {
            if ch == '{' {
                depth += 1;
                i += 1;
                continue;
            }
            if ch == '}' {
                depth -= 1;
                if depth < skip_depth {
                    skip_until_depth = None;
                }
                i += 1;
                continue;
            }
            i += 1;
            continue;
        }

        if ch == '{' {
            depth += 1;
            i += 1;

            // Check if this is a destination group to skip
            if i < len && chars[i] == '\\' {
                let word = read_control_word(&chars, i);
                let skip_destinations = [
                    "\\fonttbl", "\\colortbl", "\\stylesheet", "\\info",
                    "\\header", "\\footer", "\\pict", "\\object",
                    "\\*",
                ];
                for dest in &skip_destinations {
                    if word.starts_with(dest) {
                        skip_until_depth = Some(depth);
                        break;
                    }
                }
            }
            continue;
        }

        if ch == '}' {
            depth -= 1;
            i += 1;
            continue;
        }

        if ch == '\\' {
            // Escape sequences
            if i + 1 < len {
                let next = chars[i + 1];
                match next {
                    '\\' => { result.push('\\'); i += 2; continue; }
                    '{' => { result.push('{'); i += 2; continue; }
                    '}' => { result.push('}'); i += 2; continue; }
                    '~' => { result.push('\u{00A0}'); i += 2; continue; } // non-breaking space
                    '-' => { i += 2; continue; } // optional hyphen, skip
                    '\'' => {
                        // Hex-encoded character: \'xx
                        if i + 3 < len {
                            let hex: String = chars[i+2..i+4].iter().collect();
                            if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                                result.push(byte as char);
                            }
                            i += 4;
                        } else {
                            i += 2;
                        }
                        continue;
                    }
                    '\n' | '\r' => {
                        // Line break in RTF source after backslash = paragraph continuation
                        i += 2;
                        continue;
                    }
                    _ => {}
                }

                // Read control word
                if next.is_ascii_alphabetic() {
                    let word = read_control_word(&chars, i);
                    let word_len = word.len();

                    // Handle known control words
                    let base_word = word.trim_end_matches(|c: char| c.is_ascii_digit() || c == '-' || c == ' ');
                    match base_word {
                        "\\par" | "\\pard" => {
                            result.push('\n');
                        }
                        "\\line" => {
                            result.push('\n');
                        }
                        "\\tab" => {
                            result.push('\t');
                        }
                        _ => {
                            // Skip unknown control words silently
                        }
                    }

                    i += word_len;
                    // Skip optional space delimiter after control word
                    if i < len && chars[i] == ' ' {
                        i += 1;
                    }
                    continue;
                }
            }

            i += 1;
            continue;
        }

        // Regular character - only add if not at root level (depth 0 is outside the RTF envelope)
        if ch == '\n' || ch == '\r' {
            // RTF source line breaks are not meaningful
            i += 1;
            continue;
        }

        result.push(ch);
        i += 1;
    }

    // Clean up: collapse multiple newlines into max 2
    let mut cleaned = String::new();
    let mut consecutive_newlines = 0u32;
    for ch in result.chars() {
        if ch == '\n' {
            consecutive_newlines += 1;
            if consecutive_newlines <= 2 {
                cleaned.push(ch);
            }
        } else {
            consecutive_newlines = 0;
            cleaned.push(ch);
        }
    }

    cleaned.trim().to_string()
}

/// Read a control word starting at position i (which should point to the backslash).
/// Returns the control word including the backslash, any alphabetic chars, and optional numeric parameter.
fn read_control_word(chars: &[char], start: usize) -> String {
    let mut i = start;
    let len = chars.len();

    if i >= len || chars[i] != '\\' {
        return String::new();
    }

    let mut word = String::new();
    word.push('\\');
    i += 1;

    // Read alphabetic part
    while i < len && chars[i].is_ascii_alphabetic() {
        word.push(chars[i]);
        i += 1;
    }

    // Read optional numeric parameter (including negative sign)
    if i < len && (chars[i] == '-' || chars[i].is_ascii_digit()) {
        word.push(chars[i]);
        i += 1;
        while i < len && chars[i].is_ascii_digit() {
            word.push(chars[i]);
            i += 1;
        }
    }

    word
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_simple_rtf() {
        let rtf = r"{\rtf1\ansi Hello world}";
        let text = extract_rtf_text(rtf);
        assert_eq!(text, "Hello world");
    }

    #[test]
    fn test_extract_with_par() {
        let rtf = r"{\rtf1\ansi First paragraph.\par Second paragraph.}";
        let text = extract_rtf_text(rtf);
        assert!(text.contains("First paragraph."));
        assert!(text.contains("Second paragraph."));
    }

    #[test]
    fn test_extract_skips_fonttbl() {
        let rtf = r"{\rtf1{\fonttbl{\f0 Times New Roman;}}Hello}";
        let text = extract_rtf_text(rtf);
        assert_eq!(text, "Hello");
    }

    #[test]
    fn test_extract_escapes() {
        let rtf = r"{\rtf1 A \{ B \} C \\ D}";
        let text = extract_rtf_text(rtf);
        assert!(text.contains("A { B } C \\ D"));
    }

    #[test]
    fn test_rtf_parser_parse() {
        let dir = tempfile::TempDir::new().unwrap();
        let file = dir.path().join("test.rtf");
        std::fs::write(&file, r"{\rtf1\ansi Hello world.\par Second paragraph.}").unwrap();

        let parser = RtfParser;
        let blocks = super::super::FormatParser::parse(&parser, &file).unwrap();
        assert!(!blocks.is_empty());
        // Should have extracted text
        let all_text: String = blocks.iter().map(|b| b.text.as_str()).collect::<Vec<_>>().join(" ");
        assert!(all_text.contains("Hello world"));
        assert!(all_text.contains("Second paragraph"));
    }

    #[test]
    fn test_rtf_parser_not_rtf() {
        let dir = tempfile::TempDir::new().unwrap();
        let file = dir.path().join("test.rtf");
        std::fs::write(&file, "Not an RTF file").unwrap();

        let parser = RtfParser;
        let result = super::super::FormatParser::parse(&parser, &file);
        assert!(result.is_err());
    }
}
