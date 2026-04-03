use parser_chunker::model::{Block, ElementType};

#[test]
fn block_creation() {
    let block = Block::new(ElementType::NarrativeText, "Hello, world.");
    assert_eq!(block.text, "Hello, world.");
    assert_eq!(block.page, 0);
    assert_eq!(block.confidence, 1.0);
    assert!(block.bbox.is_none());
    assert!(block.table_data.is_none());
}

#[test]
fn format_detection() {
    use std::path::Path;
    use parser_chunker::format::detect_format;

    assert_eq!(detect_format(Path::new("test.pdf")), Some("pdf"));
    assert_eq!(detect_format(Path::new("test.docx")), Some("docx"));
    assert_eq!(detect_format(Path::new("test.xlsx")), Some("xlsx"));
    assert_eq!(detect_format(Path::new("test.html")), Some("html"));
    assert_eq!(detect_format(Path::new("test.csv")), Some("csv_tsv"));
    assert_eq!(detect_format(Path::new("test.txt")), Some("text"));
    assert_eq!(detect_format(Path::new("test.unknown")), None);
}
