use std::io::Cursor;
use std::path::Path;
use std::sync::{Mutex, Once};

use crate::model::block::{Block, BoundingBox, TableData};
use crate::model::element::ElementType;

pub struct PdfParser;

/// A wrapper so we can store `Pdfium` in a `Mutex` behind a `Once`.
/// `Pdfium` in pdfium-render 0.8 has `unsafe impl Send` and `unsafe impl Sync`
/// when `thread_safe` feature is on, but the compiler doesn't see through
/// `Box<dyn PdfiumLibraryBindings>` automatically.
struct PdfiumHolder(pdfium_render::prelude::Pdfium);
// SAFETY: Pdfium has `unsafe impl Send for Pdfium` and `unsafe impl Sync for Pdfium`
// when `thread_safe` feature is enabled (which it is via default features).
unsafe impl Send for PdfiumHolder {}
unsafe impl Sync for PdfiumHolder {}

static PDFIUM_INIT: Once = Once::new();
static PDFIUM_HOLDER: Mutex<Option<PdfiumHolder>> = Mutex::new(None);
/// Track whether init was attempted (to avoid retrying on failure).
static PDFIUM_TRIED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Try to get a `Pdfium` handle, initialising on first call.
/// Returns `None` when PDFium could not be loaded (e.g. missing binary).
fn with_pdfium<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&pdfium_render::prelude::Pdfium) -> R,
{
    PDFIUM_INIT.call_once(|| {
        PDFIUM_TRIED.store(true, std::sync::atomic::Ordering::Relaxed);
        match pdfium_auto::bind_pdfium_silent() {
            Ok(p) => {
                tracing::info!("PDFium engine loaded successfully (pdfium-auto)");
                *PDFIUM_HOLDER.lock().unwrap() = Some(PdfiumHolder(p));
            }
            Err(e) => {
                tracing::warn!("PDFium unavailable, falling back to pdf-extract: {e}");
            }
        }
    });
    let guard = PDFIUM_HOLDER.lock().unwrap();
    guard.as_ref().map(|h| f(&h.0))
}

// ── FormatParser impl ────────────────────────────────────────────────────────

impl super::FormatParser for PdfParser {
    fn parse(&self, path: &Path) -> Result<Vec<Block>, crate::Error> {
        let pdfium_result = with_pdfium(|pdfium| parse_with_pdfium(pdfium, path));

        if let Some(Ok(blocks)) = pdfium_result {
            return Ok(blocks);
        }
        if let Some(Err(e)) = pdfium_result {
            tracing::warn!(
                "PDFium extraction failed for {:?}, falling back to pdf-extract: {e}",
                path
            );
        }
        parse_with_pdf_extract(path)
    }

    fn supported_extensions(&self) -> &[&str] {
        &["pdf"]
    }
}

// ── PDFium path (high-quality) ───────────────────────────────────────────────

fn parse_with_pdfium(
    pdfium: &pdfium_render::prelude::Pdfium,
    path: &Path,
) -> Result<Vec<Block>, crate::Error> {
    let document = pdfium
        .load_pdf_from_file(path, None)
        .map_err(|e| crate::Error::Parse(format!("PDFium: {e}")))?;

    let mut blocks = Vec::new();
    let page_count = document.pages().len();

    for page_idx in 0..page_count {
        let page = document
            .pages()
            .get(page_idx)
            .map_err(|e| crate::Error::Parse(format!("PDFium page {page_idx}: {e}")))?;

        let page_num = (page_idx as u32) + 1;

        let text_page = page
            .text()
            .map_err(|e| crate::Error::Parse(format!("PDFium text page {page_num}: {e}")))?;

        // Collect character-level info for heading detection and table heuristics
        let char_infos = collect_char_info(&text_page);

        if char_infos.is_empty() {
            continue;
        }

        // Group characters into lines by baseline proximity
        let lines = group_into_lines(&char_infos);

        // Detect the dominant (body) font size across the page
        let body_font_size = detect_body_font_size(&char_infos);

        // Detect table regions (aligned columns)
        let table_regions = detect_table_regions(&lines);

        // Build blocks from lines, merging consecutive body-text lines into paragraphs
        let page_blocks = build_blocks_from_lines(
            &lines,
            &table_regions,
            body_font_size,
            page_num,
        );

        blocks.extend(page_blocks);
    }

    if blocks.is_empty() {
        tracing::warn!(
            "PDFium: PDF at {:?} produced no extractable text (may be scanned/image-only)",
            path
        );
    }

    Ok(blocks)
}

// ── Character-level info ─────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct CharInfo {
    ch: char,
    font_size: f32,
    font_name: String,
    is_bold: bool,
    x: f32,
    y_bottom: f32,
    y_top: f32,
    width: f32,
}

fn collect_char_info(text_page: &pdfium_render::prelude::PdfPageText<'_>) -> Vec<CharInfo> {
    let mut infos = Vec::new();

    for ch in text_page.chars().iter() {
        let unicode_char = match ch.unicode_char() {
            Some(c) => c,
            None => continue,
        };

        let font_size = ch.unscaled_font_size().value;
        if font_size <= 0.0 {
            // Skip zero-size chars (control/spacing)
            if !unicode_char.is_whitespace() {
                continue;
            }
        }

        let font_name = ch.font_name();
        let is_bold = font_name.to_lowercase().contains("bold")
            || ch.font_weight().is_some_and(is_bold_weight);

        // Get character bounding box
        let (x, y_bottom, y_top, width) = if let Ok(bounds) = ch.tight_bounds() {
            (
                bounds.left().value,
                bounds.bottom().value,
                bounds.top().value,
                bounds.width().value,
            )
        } else {
            // Fallback for whitespace or chars without bounds
            infos.push(CharInfo {
                ch: unicode_char,
                font_size,
                font_name,
                is_bold,
                x: 0.0,
                y_bottom: 0.0,
                y_top: 0.0,
                width: 0.0,
            });
            continue;
        };

        infos.push(CharInfo {
            ch: unicode_char,
            font_size,
            font_name,
            is_bold,
            x,
            y_bottom,
            y_top,
            width,
        });
    }

    infos
}

fn is_bold_weight(w: pdfium_render::prelude::PdfFontWeight) -> bool {
    use pdfium_render::prelude::PdfFontWeight;
    matches!(
        w,
        PdfFontWeight::Weight600
            | PdfFontWeight::Weight700Bold
            | PdfFontWeight::Weight800
            | PdfFontWeight::Weight900
    ) || matches!(w, PdfFontWeight::Custom(v) if v >= 600)
}

// ── Line grouping ────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct TextLine {
    chars: Vec<CharInfo>,
    y_baseline: f32,
    x_start: f32,
    x_end: f32,
    dominant_font_size: f32,
    text: String,
}

fn group_into_lines(chars: &[CharInfo]) -> Vec<TextLine> {
    if chars.is_empty() {
        return Vec::new();
    }

    // Sort characters by Y (top-to-bottom), then X (left-to-right).
    // PDF coordinates have Y increasing upward, so we negate Y for top-to-bottom.
    let mut sorted: Vec<&CharInfo> = chars.iter().collect();
    sorted.sort_by(|a, b| {
        b.y_bottom
            .partial_cmp(&a.y_bottom)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(
                a.x.partial_cmp(&b.x)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
    });

    let mut lines: Vec<TextLine> = Vec::new();
    let mut current_line_chars: Vec<CharInfo> = Vec::new();
    let mut current_baseline: f32 = sorted[0].y_bottom;

    for ci in sorted {
        // Characters on the same line have similar y_bottom (within half a typical char height)
        let tolerance = ci.font_size.max(4.0) * 0.5;
        if (ci.y_bottom - current_baseline).abs() <= tolerance && !current_line_chars.is_empty() {
            current_line_chars.push(ci.clone());
        } else if current_line_chars.is_empty() {
            current_baseline = ci.y_bottom;
            current_line_chars.push(ci.clone());
        } else {
            // Finish previous line
            lines.push(finish_line(&mut current_line_chars));
            current_baseline = ci.y_bottom;
            current_line_chars.push(ci.clone());
        }
    }
    if !current_line_chars.is_empty() {
        lines.push(finish_line(&mut current_line_chars));
    }

    lines
}

fn finish_line(chars: &mut Vec<CharInfo>) -> TextLine {
    // Sort by x position within the line
    chars.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal));

    let text = build_line_text(chars);
    let y_baseline = chars
        .iter()
        .map(|c| c.y_bottom)
        .sum::<f32>()
        / chars.len() as f32;
    let x_start = chars
        .iter()
        .map(|c| c.x)
        .fold(f32::INFINITY, f32::min);
    let x_end = chars
        .iter()
        .map(|c| c.x + c.width)
        .fold(f32::NEG_INFINITY, f32::max);

    // Dominant font size = most common font size in the line
    let dominant_font_size = dominant_size(chars);

    let result = TextLine {
        chars: chars.clone(),
        y_baseline,
        x_start,
        x_end,
        dominant_font_size,
        text,
    };

    chars.clear();
    result
}

fn build_line_text(chars: &[CharInfo]) -> String {
    let mut text = String::new();
    let mut prev_x_end: Option<f32> = None;

    for ci in chars {
        // Insert space if there's a gap between characters
        if let Some(prev_end) = prev_x_end {
            let gap = ci.x - prev_end;
            let space_threshold = ci.font_size * 0.25;
            if gap > space_threshold {
                text.push(' ');
            }
        }
        text.push(ci.ch);
        prev_x_end = Some(ci.x + ci.width);
    }

    text
}

fn dominant_size(chars: &[CharInfo]) -> f32 {
    use std::collections::HashMap;
    let mut counts: HashMap<i32, usize> = HashMap::new();
    for c in chars {
        // Quantise to tenths of a point
        let key = (c.font_size * 10.0) as i32;
        *counts.entry(key).or_insert(0) += 1;
    }
    counts
        .into_iter()
        .max_by_key(|&(_, count)| count)
        .map(|(key, _)| key as f32 / 10.0)
        .unwrap_or(12.0)
}

fn detect_body_font_size(chars: &[CharInfo]) -> f32 {
    dominant_size(chars)
}

// ── Table detection ──────────────────────────────────────────────────────────

/// A contiguous range of line indices that form a table.
#[derive(Debug, Clone)]
struct TableRegion {
    start_line: usize,
    end_line: usize, // exclusive
    columns: Vec<f32>, // approximate x-positions of column separators
}

fn detect_table_regions(lines: &[TextLine]) -> Vec<TableRegion> {
    // Heuristic: a table region is a run of 3+ consecutive lines where each line
    // has 3+ "tab-stop" aligned segments (large inter-word gaps at similar x-positions).
    let mut regions = Vec::new();

    let tab_stops: Vec<Vec<f32>> = lines.iter().map(find_tab_stops).collect();

    let mut i = 0;
    while i < lines.len() {
        if tab_stops[i].len() >= 2 {
            // Potential table start — check if subsequent lines have similar tab stops
            let ref_stops = &tab_stops[i];
            let mut j = i + 1;
            while j < lines.len() && stops_align(ref_stops, &tab_stops[j], 15.0) {
                j += 1;
            }
            let run_len = j - i;
            if run_len >= 3 {
                regions.push(TableRegion {
                    start_line: i,
                    end_line: j,
                    columns: ref_stops.clone(),
                });
                i = j;
                continue;
            }
        }
        i += 1;
    }

    regions
}

/// Find x-positions of large gaps within a line (potential column separators).
fn find_tab_stops(line: &TextLine) -> Vec<f32> {
    let mut stops = Vec::new();
    let chars = &line.chars;
    if chars.len() < 2 {
        return stops;
    }

    let avg_char_width: f32 = chars.iter().map(|c| c.width).sum::<f32>() / chars.len() as f32;
    let gap_threshold = avg_char_width.max(3.0) * 3.0;

    let mut prev_end = chars[0].x + chars[0].width;
    for ci in &chars[1..] {
        let gap = ci.x - prev_end;
        if gap > gap_threshold {
            stops.push((prev_end + ci.x) / 2.0); // midpoint of gap
        }
        prev_end = ci.x + ci.width;
    }

    stops
}

/// Check if two sets of tab stops are approximately aligned.
fn stops_align(a: &[f32], b: &[f32], tolerance: f32) -> bool {
    if a.is_empty() || b.is_empty() {
        return false;
    }
    // At least half of b's stops should be close to an a stop
    let matches = b
        .iter()
        .filter(|bx| a.iter().any(|ax| (*ax - **bx).abs() < tolerance))
        .count();
    matches * 2 >= b.len()
}

// ── Block building ───────────────────────────────────────────────────────────

fn build_blocks_from_lines(
    lines: &[TextLine],
    table_regions: &[TableRegion],
    body_font_size: f32,
    page_num: u32,
) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        // Check if this line starts a table region
        if let Some(region) = table_regions.iter().find(|r| r.start_line == i) {
            // Emit a table block
            let table_block = build_table_block(lines, region, page_num);
            blocks.push(table_block);
            i = region.end_line;
            continue;
        }

        // Skip lines that are inside a table region
        if table_regions.iter().any(|r| i >= r.start_line && i < r.end_line) {
            i += 1;
            continue;
        }

        let line = &lines[i];
        let trimmed = line.text.trim();

        if trimmed.is_empty() {
            i += 1;
            continue;
        }

        // Classify the line
        let heading_ratio = line.dominant_font_size / body_font_size;
        let is_bold_line = line.chars.iter().filter(|c| c.is_bold).count() * 2
            > line.chars.len();

        if classify_as_heading(trimmed, heading_ratio, is_bold_line) {
            let mut block = Block::new(ElementType::Title, trimmed);
            block.page = page_num;
            block.confidence = 0.9;
            block.bbox = Some(line_bbox(line));
            block
                .metadata
                .insert("font_size".into(), line.dominant_font_size.into());
            if let Some(first_char) = line.chars.first() {
                block.metadata.insert(
                    "font_name".into(),
                    serde_json::Value::String(first_char.font_name.clone()),
                );
            }
            blocks.push(block);
            i += 1;
        } else if classify_as_list_item(trimmed) {
            let mut block = Block::new(ElementType::ListItem, trimmed);
            block.page = page_num;
            block.confidence = 0.9;
            block.bbox = Some(line_bbox(line));
            blocks.push(block);
            i += 1;
        } else if classify_as_footer(trimmed) {
            let mut block = Block::new(ElementType::Footer, trimmed);
            block.page = page_num;
            block.confidence = 0.9;
            blocks.push(block);
            i += 1;
        } else {
            // Narrative text: merge consecutive body-text lines into a paragraph
            let para_start = i;
            let mut para_text = String::from(trimmed);
            let mut para_y_min = line.y_baseline;
            let mut para_y_max = line
                .chars
                .iter()
                .map(|c| c.y_top)
                .fold(f32::NEG_INFINITY, f32::max);
            let mut para_x_min = line.x_start;
            let mut para_x_max = line.x_end;

            i += 1;
            while i < lines.len() {
                let next = &lines[i];
                let next_trimmed = next.text.trim();
                if next_trimmed.is_empty() {
                    break;
                }
                // Stop merging if the next line is a heading, list, footer, or in a table
                let next_heading_ratio = next.dominant_font_size / body_font_size;
                let next_is_bold = next.chars.iter().filter(|c| c.is_bold).count() * 2
                    > next.chars.len();
                if classify_as_heading(next_trimmed, next_heading_ratio, next_is_bold)
                    || classify_as_list_item(next_trimmed)
                    || classify_as_footer(next_trimmed)
                    || table_regions.iter().any(|r| i >= r.start_line && i < r.end_line)
                {
                    break;
                }
                // Also stop if there's a large vertical gap (paragraph break)
                let line_spacing = (lines[i - 1].y_baseline - next.y_baseline).abs();
                let expected_spacing = body_font_size * 1.5;
                if line_spacing > expected_spacing * 1.8 && i > para_start + 1 {
                    break;
                }

                para_text.push(' ');
                para_text.push_str(next_trimmed);
                para_y_min = para_y_min.min(next.y_baseline);
                para_y_max = para_y_max.max(
                    next.chars
                        .iter()
                        .map(|c| c.y_top)
                        .fold(f32::NEG_INFINITY, f32::max),
                );
                para_x_min = para_x_min.min(next.x_start);
                para_x_max = para_x_max.max(next.x_end);
                i += 1;
            }

            let mut block = Block::new(ElementType::NarrativeText, &para_text);
            block.page = page_num;
            block.confidence = 0.9;
            block.bbox = Some(BoundingBox {
                x: para_x_min,
                y: para_y_min,
                width: para_x_max - para_x_min,
                height: para_y_max - para_y_min,
            });
            blocks.push(block);
        }
    }

    blocks
}

fn line_bbox(line: &TextLine) -> BoundingBox {
    let y_top = line
        .chars
        .iter()
        .map(|c| c.y_top)
        .fold(f32::NEG_INFINITY, f32::max);
    BoundingBox {
        x: line.x_start,
        y: line.y_baseline,
        width: line.x_end - line.x_start,
        height: y_top - line.y_baseline,
    }
}

fn build_table_block(lines: &[TextLine], region: &TableRegion, page_num: u32) -> Block {
    let mut rows: Vec<Vec<String>> = Vec::new();

    for line in lines.iter().take(region.end_line).skip(region.start_line) {
        let mut cells: Vec<String> = Vec::new();

        // Split the line text at column boundaries
        let mut col_chars: Vec<Vec<&CharInfo>> = vec![Vec::new(); region.columns.len() + 1];
        for ci in &line.chars {
            let col_idx = region
                .columns
                .iter()
                .position(|&sep| ci.x < sep)
                .unwrap_or(region.columns.len());
            col_chars[col_idx].push(ci);
        }

        for col in &col_chars {
            let cell_text: String = col.iter().map(|c| c.ch).collect();
            cells.push(cell_text.trim().to_string());
        }
        rows.push(cells);
    }

    // First row might be headers if it's bold or has different styling
    let headers = if !rows.is_empty() {
        let first_line = &lines[region.start_line];
        let is_header_bold = first_line.chars.iter().filter(|c| c.is_bold).count() * 2
            > first_line.chars.len();
        if is_header_bold {
            Some(rows.remove(0))
        } else {
            None
        }
    } else {
        None
    };

    let table_text = rows
        .iter()
        .map(|r| r.join(" | "))
        .collect::<Vec<_>>()
        .join("\n");

    let mut block = Block::new(ElementType::Table, &table_text);
    block.page = page_num;
    block.confidence = 0.85;
    block.table_data = Some(TableData { rows, headers });
    block
}

fn classify_as_heading(text: &str, size_ratio: f32, is_bold: bool) -> bool {
    let line_count = text.lines().count();
    if line_count > 2 {
        return false;
    }
    if text.len() > 200 {
        return false;
    }
    // Significantly larger font = heading
    if size_ratio >= 1.2 {
        return true;
    }
    // Bold + short + no trailing period = heading
    if is_bold && text.len() < 120 && !text.ends_with('.') {
        return true;
    }
    // ALL CAPS short text
    if line_count == 1 && text.len() < 80 && !text.ends_with('.') {
        let alpha: Vec<char> = text.chars().filter(|c| c.is_alphabetic()).collect();
        if !alpha.is_empty() {
            let upper_ratio =
                alpha.iter().filter(|c| c.is_uppercase()).count() as f64 / alpha.len() as f64;
            if upper_ratio > 0.7 {
                return true;
            }
        }
    }
    false
}

fn classify_as_list_item(text: &str) -> bool {
    text.starts_with("- ")
        || text.starts_with("* ")
        || text.starts_with("• ")
        || text.starts_with("· ")
        || text.starts_with("◦ ")
        || text.starts_with("‣ ")
        || text
            .split_once(['.', ')'])
            .is_some_and(|(prefix, _)| {
                !prefix.is_empty() && prefix.len() <= 3 && prefix.chars().all(|c| c.is_ascii_digit())
            })
}

fn classify_as_footer(text: &str) -> bool {
    let line_count = text.lines().count();
    if line_count != 1 {
        return false;
    }
    text.parse::<u32>().is_ok() || text.to_lowercase().starts_with("page ")
}

// ── pdf-extract fallback ─────────────────────────────────────────────────────

fn parse_with_pdf_extract(path: &Path) -> Result<Vec<Block>, crate::Error> {
    tracing::info!("Using pdf-extract fallback for {:?}", path);

    let pages = pdf_extract::extract_text_by_pages(path).map_err(|e| {
        let msg = e.to_string();
        if msg.contains("decrypt") || msg.contains("encrypt") || msg.contains("password") {
            crate::Error::Parse(format!("PDF is encrypted or password-protected: {msg}"))
        } else {
            crate::Error::Parse(format!("PDF extraction failed: {msg}"))
        }
    })?;

    let mut blocks = Vec::new();

    for (page_idx, page_text) in pages.iter().enumerate() {
        let page_num = (page_idx as u32) + 1;
        let trimmed = page_text.trim();
        if trimmed.is_empty() {
            continue;
        }

        let paragraphs: Vec<&str> = trimmed.split("\n\n").collect();

        for para in paragraphs {
            let para_text = para.trim();
            if para_text.is_empty() {
                continue;
            }

            let element_type = classify_paragraph_fallback(para_text);

            let mut block = Block::new(element_type, para_text);
            block.page = page_num;
            block.confidence = 0.7;
            blocks.push(block);
        }
    }

    if blocks.is_empty() {
        tracing::warn!(
            "PDF at {:?} produced no extractable text (may be scanned/image-only)",
            path
        );
    }

    Ok(blocks)
}

/// Simple heuristic classification for the pdf-extract fallback path.
fn classify_paragraph_fallback(text: &str) -> ElementType {
    let line_count = text.lines().count();
    let char_count = text.len();

    if line_count == 1 && (text.parse::<u32>().is_ok() || text.to_lowercase().starts_with("page "))
    {
        return ElementType::Footer;
    }

    if classify_as_list_item(text) {
        return ElementType::ListItem;
    }

    if line_count == 1 && char_count < 120 && !text.ends_with('.') {
        let alpha_count = text.chars().filter(|c| c.is_alphabetic()).count();
        if alpha_count > 0 {
            let upper_ratio = text
                .chars()
                .filter(|c| c.is_alphabetic())
                .filter(|c| c.is_uppercase())
                .count() as f64
                / alpha_count as f64;

            if upper_ratio > 0.6 || char_count < 60 {
                return ElementType::Title;
            }
        }
    }

    ElementType::NarrativeText
}

// ── Page rasterization (for vision/OCR pipeline) ─────────────────────────────

/// Rasterize a single PDF page to PNG bytes at the given DPI.
///
/// Returns `Ok(png_bytes)` on success. This uses PDFium for high-quality rendering.
/// Returns an error if PDFium is not available or the page cannot be rendered.
pub fn rasterize_page(path: &Path, page_num: u32, dpi: u32) -> Result<Vec<u8>, crate::Error> {
    use pdfium_render::prelude::PdfRenderConfig;

    let result = with_pdfium(|pdfium| -> Result<Vec<u8>, crate::Error> {
        let document = pdfium
            .load_pdf_from_file(path, None)
            .map_err(|e| crate::Error::Parse(format!("PDFium: {e}")))?;

        let page_index = page_num.checked_sub(1).ok_or_else(|| {
            crate::Error::Parse("page_num must be >= 1".into())
        })?;

        let page_index_u16: u16 = page_index.try_into().map_err(|_| {
            crate::Error::Parse(format!("Page index {page_index} exceeds u16 range"))
        })?;

        let page = document
            .pages()
            .get(page_index_u16)
            .map_err(|e| crate::Error::Parse(format!("PDFium page {page_num}: {e}")))?;

        // Calculate pixel dimensions from DPI and page size in points (72 points = 1 inch)
        let scale = dpi as f32 / 72.0;
        let width_px = (page.width().value * scale) as i32;
        let height_px = (page.height().value * scale) as i32;

        let config = PdfRenderConfig::new()
            .set_target_width(width_px)
            .set_target_height(height_px);

        let bitmap = page
            .render_with_config(&config)
            .map_err(|e| crate::Error::Parse(format!("PDFium render: {e}")))?;

        let image = bitmap.as_image();

        let mut png_bytes = Vec::new();
        image
            .write_to(&mut Cursor::new(&mut png_bytes), image::ImageFormat::Png)
            .map_err(|e| crate::Error::Parse(format!("PNG encoding: {e}")))?;

        Ok(png_bytes)
    });

    match result {
        Some(inner) => inner,
        None => Err(crate::Error::Parse(
            "PDFium is not available; page rasterization requires PDFium".into(),
        )),
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_short_uppercase_as_title() {
        assert!(classify_as_heading("INTRODUCTION", 1.3, false));
    }

    #[test]
    fn classify_bold_short_as_heading() {
        assert!(classify_as_heading("Methods and Results", 1.0, true));
    }

    #[test]
    fn classify_long_text_not_heading() {
        let long = "This is a long paragraph of text that describes something in detail. It has multiple sentences and ends with a period.";
        assert!(!classify_as_heading(long, 1.0, false));
    }

    #[test]
    fn classify_footer_works() {
        assert!(classify_as_footer("42"));
        assert!(classify_as_footer("Page 7"));
        assert!(!classify_as_footer("Some text here"));
    }

    #[test]
    fn classify_list_item_works() {
        assert!(classify_as_list_item("- First item"));
        assert!(classify_as_list_item("• Bullet point"));
        assert!(classify_as_list_item("1. Numbered item"));
        assert!(classify_as_list_item("2) Another item"));
        assert!(!classify_as_list_item("Regular text"));
    }

    #[test]
    fn fallback_classify_paragraph() {
        assert_eq!(
            classify_paragraph_fallback("INTRODUCTION"),
            ElementType::Title
        );
        assert_eq!(classify_paragraph_fallback("42"), ElementType::Footer);
        assert_eq!(
            classify_paragraph_fallback("- First item\n- Second item"),
            ElementType::ListItem
        );
    }

    #[test]
    fn tab_stop_detection() {
        // Create fake line with large gap
        let chars = vec![
            CharInfo {
                ch: 'A',
                font_size: 12.0,
                font_name: "Arial".into(),
                is_bold: false,
                x: 10.0,
                y_bottom: 100.0,
                y_top: 112.0,
                width: 8.0,
            },
            CharInfo {
                ch: 'B',
                font_size: 12.0,
                font_name: "Arial".into(),
                is_bold: false,
                x: 200.0,
                y_bottom: 100.0,
                y_top: 112.0,
                width: 8.0,
            },
        ];
        let line = TextLine {
            text: "A B".into(),
            chars,
            y_baseline: 100.0,
            x_start: 10.0,
            x_end: 208.0,
            dominant_font_size: 12.0,
        };
        let stops = find_tab_stops(&line);
        assert!(!stops.is_empty());
    }
}
