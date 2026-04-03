pub mod csv_tsv;
pub mod docx;
pub mod email;
pub mod html;
pub mod image;
pub mod msg;
pub mod pdf;
pub mod pptx;
pub mod rtf;
pub mod text;
pub mod xlsx;

use std::path::Path;

use crate::model::Block;

/// Trait implemented by each format-specific parser.
pub trait FormatParser: Send + Sync {
    fn parse(&self, path: &Path) -> Result<Vec<Block>, crate::Error>;
    fn supported_extensions(&self) -> &[&str];
}

/// Detect format from extension (fast path).
/// Uses `eq_ignore_ascii_case` to avoid allocating a lowercase String.
pub fn detect_format(path: &Path) -> Option<&'static str> {
    let ext = path.extension()?.to_str()?;
    // Match using case-insensitive comparison without allocation
    detect_format_from_ext(ext)
}

/// Map an extension string (case-insensitive) to a format key without allocating.
fn detect_format_from_ext(ext: &str) -> Option<&'static str> {
    // Common extensions first for fast-path
    if ext.eq_ignore_ascii_case("txt") || ext.eq_ignore_ascii_case("text")
        || ext.eq_ignore_ascii_case("log") || ext.eq_ignore_ascii_case("cfg")
        || ext.eq_ignore_ascii_case("ini") || ext.eq_ignore_ascii_case("conf")
    {
        return Some("text");
    }
    if ext.eq_ignore_ascii_case("csv") { return Some("csv_tsv"); }
    if ext.eq_ignore_ascii_case("html") || ext.eq_ignore_ascii_case("htm")
        || ext.eq_ignore_ascii_case("xhtml") || ext.eq_ignore_ascii_case("mhtml")
        || ext.eq_ignore_ascii_case("mht")
    {
        return Some("html");
    }
    if ext.eq_ignore_ascii_case("md") || ext.eq_ignore_ascii_case("markdown")
        || ext.eq_ignore_ascii_case("rst")
    {
        return Some("text");
    }
    if ext.eq_ignore_ascii_case("pdf") { return Some("pdf"); }
    if ext.eq_ignore_ascii_case("docx") || ext.eq_ignore_ascii_case("doc") { return Some("docx"); }
    if ext.eq_ignore_ascii_case("xlsx") || ext.eq_ignore_ascii_case("xlsb")
        || ext.eq_ignore_ascii_case("xls") || ext.eq_ignore_ascii_case("ods")
    {
        return Some("xlsx");
    }
    if ext.eq_ignore_ascii_case("pptx") || ext.eq_ignore_ascii_case("ppt") { return Some("pptx"); }
    if ext.eq_ignore_ascii_case("eml") { return Some("email"); }
    if ext.eq_ignore_ascii_case("msg") { return Some("msg"); }
    if ext.eq_ignore_ascii_case("tsv") || ext.eq_ignore_ascii_case("tab") { return Some("csv_tsv"); }
    if ext.eq_ignore_ascii_case("rtf") { return Some("rtf"); }
    if ext.eq_ignore_ascii_case("xml") || ext.eq_ignore_ascii_case("xsd")
        || ext.eq_ignore_ascii_case("xsl") || ext.eq_ignore_ascii_case("svg")
        || ext.eq_ignore_ascii_case("rss") || ext.eq_ignore_ascii_case("atom")
    {
        return Some("xml");
    }
    if ext.eq_ignore_ascii_case("json") || ext.eq_ignore_ascii_case("jsonl")
        || ext.eq_ignore_ascii_case("ndjson")
    {
        return Some("json");
    }
    if ext.eq_ignore_ascii_case("yaml") || ext.eq_ignore_ascii_case("yml") { return Some("yaml"); }
    if ext.eq_ignore_ascii_case("epub") { return Some("epub"); }
    if ext.eq_ignore_ascii_case("png") || ext.eq_ignore_ascii_case("jpg")
        || ext.eq_ignore_ascii_case("jpeg") || ext.eq_ignore_ascii_case("tiff")
        || ext.eq_ignore_ascii_case("tif") || ext.eq_ignore_ascii_case("bmp")
        || ext.eq_ignore_ascii_case("gif") || ext.eq_ignore_ascii_case("webp")
        || ext.eq_ignore_ascii_case("heic")
    {
        return Some("image");
    }
    None
}

/// Detect format from magic bytes (fallback for extensionless files).
pub fn detect_format_by_magic(path: &Path) -> Option<&'static str> {
    let data = std::fs::read(path).ok()?;
    let header = if data.len() > 8192 { &data[..8192] } else { &data };

    if header.starts_with(b"%PDF") {
        return Some("pdf");
    }
    if header.starts_with(b"PK\x03\x04") || header.starts_with(b"PK\x05\x06") {
        return detect_zip_contents(path).or(Some("docx"));
    }
    if header.starts_with(b"\xd0\xcf\x11\xe0\xa1\xb1\x1a\xe1") {
        return Some("msg");
    }
    if header.starts_with(b"{\\rtf") {
        return Some("rtf");
    }
    if header.starts_with(b"\x89PNG") || header.starts_with(b"\xff\xd8\xff") || header.starts_with(b"GIF8") {
        return Some("image");
    }
    if header.starts_with(b"BM") && header.len() > 14 {
        return Some("image");
    }
    if header.starts_with(b"II\x2a\x00") || header.starts_with(b"MM\x00\x2a") {
        return Some("image");
    }

    if let Ok(text) = std::str::from_utf8(header) {
        let lower = text.to_lowercase();
        if lower.contains("<!doctype html") || lower.contains("<html") {
            return Some("html");
        }
        if lower.starts_with("<?xml") {
            return Some("xml");
        }
        let trimmed = text.trim_start();
        if trimmed.starts_with('{') || trimmed.starts_with('[') {
            return Some("json");
        }
    }

    None
}

fn detect_zip_contents(path: &Path) -> Option<&'static str> {
    let file = std::fs::File::open(path).ok()?;
    let mut archive = zip::ZipArchive::new(file).ok()?;
    for i in 0..archive.len().min(20) {
        if let Ok(entry) = archive.by_index(i) {
            let name = entry.name().to_lowercase();
            if name.starts_with("word/") { return Some("docx"); }
            if name.starts_with("xl/") { return Some("xlsx"); }
            if name.starts_with("ppt/") { return Some("pptx"); }
        }
    }
    None
}

/// Get the appropriate parser for a detected format key.
/// Returns a reference to a static parser instance — no allocation per call.
pub fn get_parser(format_key: &str) -> Option<&'static dyn FormatParser> {
    static TEXT: text::TextParser = text::TextParser;
    static CSV: csv_tsv::CsvTsvParser = csv_tsv::CsvTsvParser;
    static DOCX: docx::DocxParser = docx::DocxParser;
    static HTML: html::HtmlParser = html::HtmlParser;
    static EMAIL: email::EmailParser = email::EmailParser;
    static MSG: msg::MsgParser = msg::MsgParser;
    static XLSX: xlsx::XlsxParser = xlsx::XlsxParser;
    static PDF: pdf::PdfParser = pdf::PdfParser;
    static PPTX: pptx::PptxParser = pptx::PptxParser;
    static IMAGE: image::ImageParser = image::ImageParser;
    static RTF: rtf::RtfParser = rtf::RtfParser;

    match format_key {
        "text" | "json" | "yaml" | "xml" => Some(&TEXT),
        "csv_tsv" => Some(&CSV),
        "docx" => Some(&DOCX),
        "html" => Some(&HTML),
        "email" => Some(&EMAIL),
        "msg" => Some(&MSG),
        "xlsx" => Some(&XLSX),
        "pdf" => Some(&PDF),
        "pptx" => Some(&PPTX),
        "image" => Some(&IMAGE),
        "rtf" => Some(&RTF),
        _ => None,
    }
}
