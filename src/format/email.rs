use std::path::Path;

use mail_parser::{Addr, Address, Group, MessageParser, MimeHeaders};

use crate::model::{Block, ElementType};

pub struct EmailParser;

impl super::FormatParser for EmailParser {
    fn parse(&self, path: &Path) -> Result<Vec<Block>, crate::Error> {
        let raw = std::fs::read(path).map_err(|e| crate::Error::Io(e.to_string()))?;
        let message = MessageParser::default()
            .parse(&raw)
            .ok_or_else(|| crate::Error::Parse("Failed to parse email".into()))?;

        let mut blocks = Vec::new();

        // Subject as Title
        if let Some(subject) = message.subject() {
            if !subject.is_empty() {
                blocks.push(Block {
                    element_type: ElementType::Title,
                    text: subject.to_string(),
                    page: 1,
                    confidence: 0.9,
                    ..Block::default()
                });
            }
        }

        // From header
        if let Some(from) = message.from() {
            let from_str = format_address(from);
            if !from_str.is_empty() {
                blocks.push(Block {
                    element_type: ElementType::EmailHeader,
                    text: format!("From: {from_str}"),
                    page: 1,
                    confidence: 0.9,
                    ..Block::default()
                });
            }
        }

        // To header
        if let Some(to) = message.to() {
            let to_str = format_address(to);
            if !to_str.is_empty() {
                blocks.push(Block {
                    element_type: ElementType::EmailHeader,
                    text: format!("To: {to_str}"),
                    page: 1,
                    confidence: 0.9,
                    ..Block::default()
                });
            }
        }

        // Date header
        if let Some(date) = message.date() {
            blocks.push(Block {
                element_type: ElementType::EmailHeader,
                text: format!("Date: {date}"),
                page: 1,
                confidence: 1.0,
                ..Block::default()
            });
        }

        // Body — prefer plain text, fall back to HTML with tag stripping
        let body_text = message
            .body_text(0)
            .map(|t| t.to_string())
            .or_else(|| message.body_html(0).map(|h| strip_html_tags(&h)));

        if let Some(body) = body_text {
            let trimmed = body.trim().to_string();
            if !trimmed.is_empty() {
                blocks.push(Block {
                    element_type: ElementType::EmailBody,
                    text: trimmed,
                    page: 1,
                    confidence: 0.9,
                    ..Block::default()
                });
            }
        }

        // Attachments — metadata-only blocks
        let attachment_count = message.attachment_count();
        if attachment_count > 0 {
            let mut attachment_names: Vec<String> = Vec::new();
            for idx in 0..attachment_count {
                if let Some(part) = message.attachment(idx as u32) {
                    let name = part
                        .attachment_name()
                        .unwrap_or("unnamed")
                        .to_string();
                    let size = part.contents().len();
                    attachment_names.push(format!("{name} ({size} bytes)"));
                }
            }
            if !attachment_names.is_empty() {
                blocks.push(Block {
                    element_type: ElementType::NarrativeText,
                    text: format!(
                        "Attachments ({attachment_count}): {}",
                        attachment_names.join(", ")
                    ),
                    page: 1,
                    confidence: 0.9,
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
        &["eml"]
    }
}

fn format_address(addr: &Address<'_>) -> String {
    match addr {
        Address::List(list) => {
            list.iter().map(format_single_addr).collect::<Vec<_>>().join(", ")
        }
        Address::Group(groups) => {
            groups
                .iter()
                .map(format_group)
                .collect::<Vec<_>>()
                .join("; ")
        }
    }
}

fn format_group(g: &Group<'_>) -> String {
    let addrs: Vec<String> = g.addresses.iter().map(format_single_addr).collect();
    if let Some(name) = &g.name {
        format!("{name}: {}", addrs.join(", "))
    } else {
        addrs.join(", ")
    }
}

fn format_single_addr(a: &Addr<'_>) -> String {
    match (&a.name, &a.address) {
        (Some(name), Some(email)) => format!("{name} <{email}>"),
        (None, Some(email)) => email.to_string(),
        (Some(name), None) => name.to_string(),
        (None, None) => String::new(),
    }
}

/// Simple HTML tag stripper for email HTML bodies.
fn strip_html_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    let mut last_was_space = false;

    for ch in html.chars() {
        if ch == '<' {
            in_tag = true;
            continue;
        }
        if ch == '>' {
            in_tag = false;
            // Insert space at tag boundaries to avoid run-together words
            if !last_was_space {
                out.push(' ');
                last_was_space = true;
            }
            continue;
        }
        if !in_tag {
            if ch.is_whitespace() {
                if !last_was_space {
                    out.push(' ');
                    last_was_space = true;
                }
            } else {
                out.push(ch);
                last_was_space = false;
            }
        }
    }
    out.trim().to_string()
}
