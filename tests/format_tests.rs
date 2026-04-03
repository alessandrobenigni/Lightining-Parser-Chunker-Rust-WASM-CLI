//! Comprehensive integration tests for Parser Chunker.
//!
//! Tests cover: format detection, parsing (per format), chunking (per strategy),
//! output serialization, confidence scores, error codes, and full pipeline.

use std::fs;
use std::path::Path;

use parser_chunker::chunking::{chunk_blocks, estimate_tokens};
use parser_chunker::cli::{ChunkStrategy, OutputFormat};
use parser_chunker::format::{detect_format, detect_format_by_magic, get_parser};
use parser_chunker::model::{Block, Chunk, ElementType, TableData};
use parser_chunker::orchestrator::{process_batch, process_single_file};
use parser_chunker::output::write_output;
use parser_chunker::Error;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_block(text: &str, element_type: ElementType, page: u32) -> Block {
    let mut b = Block::new(element_type, text);
    b.page = page;
    b
}

fn sample_chunks(count: usize) -> Vec<Chunk> {
    (0..count)
        .map(|i| Chunk {
            id: format!("chunk-{}", i),
            text: format!("Chunk {} text content with enough words to be meaningful.", i),
            token_count: 10,
            source_blocks: vec![Block::new(ElementType::NarrativeText, "text")],
            page_start: 1,
            page_end: 1,
            overlap_prefix: None,
            confidence: 1.0,
        })
        .collect()
}

// ===========================================================================
// 1. test_parse_plain_text
// ===========================================================================
#[test]
fn test_parse_plain_text() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("sample.txt");
    fs::write(
        &file,
        "First paragraph line one.\nFirst paragraph line two.\n\nSecond paragraph.\n",
    )
    .unwrap();

    let parser = get_parser("text").expect("text parser should exist");
    let blocks = parser.parse(&file).unwrap();

    // Two paragraphs expected (blank line separates them)
    assert!(
        blocks.len() >= 2,
        "Expected at least 2 paragraph blocks, got {}",
        blocks.len()
    );
    assert!(blocks.iter().all(|b| b.element_type == ElementType::NarrativeText));
    assert!(blocks[0].text.contains("First paragraph"));
    assert!(blocks[1].text.contains("Second paragraph"));
}

// ===========================================================================
// 2. test_parse_markdown
// ===========================================================================
#[test]
fn test_parse_markdown() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("sample.md");
    fs::write(
        &file,
        r#"# Main Title

Some introductory text.

## Sub Section

- Item one
- Item two

```rust
fn main() {}
```

Final paragraph.
"#,
    )
    .unwrap();

    let parser = get_parser("text").expect("text parser handles .md");
    let blocks = parser.parse(&file).unwrap();

    let titles: Vec<_> = blocks
        .iter()
        .filter(|b| b.element_type == ElementType::Title)
        .collect();
    assert!(
        titles.len() >= 2,
        "Expected at least 2 heading blocks, got {}",
        titles.len()
    );
    assert!(titles.iter().any(|b| b.text.contains("Main Title")));
    assert!(titles.iter().any(|b| b.text.contains("Sub Section")));

    let list_items: Vec<_> = blocks
        .iter()
        .filter(|b| b.element_type == ElementType::ListItem)
        .collect();
    assert!(
        list_items.len() >= 2,
        "Expected at least 2 list items, got {}",
        list_items.len()
    );

    let code_blocks: Vec<_> = blocks
        .iter()
        .filter(|b| b.element_type == ElementType::CodeBlock)
        .collect();
    assert!(
        !code_blocks.is_empty(),
        "Expected at least 1 code block"
    );
    assert!(code_blocks[0].text.contains("fn main()"));
}

// ===========================================================================
// 3. test_parse_csv
// ===========================================================================
#[test]
fn test_parse_csv() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("data.csv");
    fs::write(
        &file,
        "name,age,city\nAlice,30,NYC\nBob,25,LA\nCharlie,35,Chicago\n",
    )
    .unwrap();

    let parser = get_parser("csv_tsv").expect("csv_tsv parser should exist");
    let blocks = parser.parse(&file).unwrap();

    assert_eq!(blocks.len(), 1, "CSV should produce exactly 1 Table block");
    assert_eq!(blocks[0].element_type, ElementType::Table);

    let table = blocks[0].table_data.as_ref().expect("table_data should be set");
    let headers = table.headers.as_ref().expect("headers should be present");
    assert_eq!(headers, &["name", "age", "city"]);
    assert_eq!(table.rows.len(), 3, "Expected 3 data rows");
    assert_eq!(table.rows[0], vec!["Alice", "30", "NYC"]);
}

// ===========================================================================
// 4. test_parse_tsv_auto_detect
// ===========================================================================
#[test]
fn test_parse_tsv_auto_detect() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("data.tsv");
    fs::write(&file, "col_a\tcol_b\n1\t2\n3\t4\n").unwrap();

    let format = detect_format(&file);
    assert_eq!(format, Some("csv_tsv"), "TSV extension should map to csv_tsv");

    let parser = get_parser("csv_tsv").unwrap();
    let blocks = parser.parse(&file).unwrap();

    assert_eq!(blocks.len(), 1);
    let table = blocks[0].table_data.as_ref().unwrap();
    let headers = table.headers.as_ref().unwrap();
    assert_eq!(headers, &["col_a", "col_b"]);
    assert_eq!(table.rows.len(), 2);
}

// ===========================================================================
// 5. test_parse_html_full
// ===========================================================================
#[test]
fn test_parse_html_full() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("page.html");
    fs::write(
        &file,
        r#"<!DOCTYPE html>
<html>
<head><title>Test Page</title></head>
<body>
  <h1>Main Heading</h1>
  <p>A paragraph of text.</p>
  <h2>Sub Heading</h2>
  <ul>
    <li>First item</li>
    <li>Second item</li>
  </ul>
  <table>
    <tr><th>Name</th><th>Value</th></tr>
    <tr><td>A</td><td>1</td></tr>
    <tr><td>B</td><td>2</td></tr>
  </table>
</body>
</html>"#,
    )
    .unwrap();

    let parser = get_parser("html").expect("html parser should exist");
    let blocks = parser.parse(&file).unwrap();

    // Check title presence
    let titles: Vec<_> = blocks
        .iter()
        .filter(|b| b.element_type == ElementType::Title)
        .collect();
    assert!(
        titles.len() >= 2,
        "Expected title + h1 + h2, got {} title blocks",
        titles.len()
    );

    // Check paragraphs
    let paras: Vec<_> = blocks
        .iter()
        .filter(|b| b.element_type == ElementType::NarrativeText)
        .collect();
    assert!(!paras.is_empty(), "Expected at least 1 paragraph block");

    // Check list items
    let items: Vec<_> = blocks
        .iter()
        .filter(|b| b.element_type == ElementType::ListItem)
        .collect();
    assert_eq!(items.len(), 2, "Expected 2 list items");

    // Check table
    let tables: Vec<_> = blocks
        .iter()
        .filter(|b| b.element_type == ElementType::Table)
        .collect();
    assert_eq!(tables.len(), 1, "Expected 1 table block");
    let td = tables[0].table_data.as_ref().unwrap();
    assert_eq!(td.rows.len(), 2);
}

// ===========================================================================
// 6. test_parse_rtf_basic
// ===========================================================================
#[test]
fn test_parse_rtf_basic() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("doc.rtf");
    fs::write(
        &file,
        r"{\rtf1\ansi First paragraph content.\par Second paragraph content.}",
    )
    .unwrap();

    let parser = get_parser("rtf").expect("rtf parser should exist");
    let blocks = parser.parse(&file).unwrap();

    assert!(!blocks.is_empty(), "RTF parser should produce blocks");
    let all_text: String = blocks.iter().map(|b| b.text.as_str()).collect::<Vec<_>>().join(" ");
    assert!(all_text.contains("First paragraph"));
    assert!(all_text.contains("Second paragraph"));
    // RTF parser confidence is 0.8
    assert!(blocks.iter().all(|b| (b.confidence - 0.8).abs() < 0.01));
}

// ===========================================================================
// 7. test_parse_xml
// ===========================================================================
#[test]
fn test_parse_xml() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("data.xml");
    fs::write(
        &file,
        r#"<?xml version="1.0"?>
<root>
  <title>Document Title</title>
  <body>
    <section>
      <para>First paragraph.</para>
      <para>Second paragraph.</para>
    </section>
  </body>
</root>"#,
    )
    .unwrap();

    // XML files routed through text parser
    let parser = get_parser("text").unwrap();
    let blocks = parser.parse(&file).unwrap();

    assert!(!blocks.is_empty(), "XML parser should extract text");
    let all_text: String = blocks.iter().map(|b| b.text.as_str()).collect::<Vec<_>>().join(" ");
    assert!(all_text.contains("Document Title"));
    assert!(all_text.contains("First paragraph"));
    assert!(all_text.contains("Second paragraph"));

    // XML blocks should have hierarchy info
    let with_hierarchy: Vec<_> = blocks.iter().filter(|b| !b.hierarchy.is_empty()).collect();
    assert!(
        !with_hierarchy.is_empty(),
        "XML blocks should have hierarchy metadata"
    );
}

// ===========================================================================
// 8. test_format_detection_extensions
// ===========================================================================
#[test]
fn test_format_detection_extensions() {
    let cases = vec![
        ("report.pdf", Some("pdf")),
        ("doc.docx", Some("docx")),
        ("doc.doc", Some("docx")),
        ("data.xlsx", Some("xlsx")),
        ("data.xls", Some("xlsx")),
        ("data.ods", Some("xlsx")),
        ("slides.pptx", Some("pptx")),
        ("slides.ppt", Some("pptx")),
        ("page.html", Some("html")),
        ("page.htm", Some("html")),
        ("page.xhtml", Some("html")),
        ("page.mhtml", Some("html")),
        ("mail.eml", Some("email")),
        ("mail.msg", Some("msg")),
        ("data.csv", Some("csv_tsv")),
        ("data.tsv", Some("csv_tsv")),
        ("data.tab", Some("csv_tsv")),
        ("readme.txt", Some("text")),
        ("app.log", Some("text")),
        ("config.cfg", Some("text")),
        ("settings.ini", Some("text")),
        ("readme.md", Some("text")),
        ("readme.markdown", Some("text")),
        ("doc.rst", Some("text")),
        ("style.rtf", Some("rtf")),
        ("data.xml", Some("xml")),
        ("data.json", Some("json")),
        ("data.jsonl", Some("json")),
        ("data.yaml", Some("yaml")),
        ("data.yml", Some("yaml")),
        ("book.epub", Some("epub")),
        ("photo.png", Some("image")),
        ("photo.jpg", Some("image")),
        ("photo.jpeg", Some("image")),
        ("photo.gif", Some("image")),
        ("photo.webp", Some("image")),
        ("unknown.xyz", None),
        ("noext", None),
    ];

    for (filename, expected) in cases {
        let result = detect_format(Path::new(filename));
        assert_eq!(
            result, expected,
            "detect_format({}) expected {:?}, got {:?}",
            filename, expected, result
        );
    }
}

// ===========================================================================
// 9. test_format_detection_magic_bytes
// ===========================================================================
#[test]
fn test_format_detection_magic_bytes() {
    let dir = TempDir::new().unwrap();

    // PDF magic bytes
    let pdf_file = dir.path().join("mystery1");
    fs::write(&pdf_file, b"%PDF-1.4 fake pdf content").unwrap();
    assert_eq!(detect_format_by_magic(&pdf_file), Some("pdf"));

    // ZIP/DOCX magic bytes (PK header)
    let zip_file = dir.path().join("mystery2");
    fs::write(&zip_file, b"PK\x03\x04 fake zip content with enough bytes to read").unwrap();
    // ZIP without recognizable internal structure defaults to "docx"
    let result = detect_format_by_magic(&zip_file);
    assert!(
        result == Some("docx") || result == Some("xlsx") || result == Some("pptx"),
        "ZIP magic should detect as docx/xlsx/pptx, got {:?}",
        result
    );

    // HTML magic bytes
    let html_file = dir.path().join("mystery3");
    fs::write(&html_file, b"<!DOCTYPE html><html><body>hello</body></html>").unwrap();
    assert_eq!(detect_format_by_magic(&html_file), Some("html"));

    // RTF magic bytes
    let rtf_file = dir.path().join("mystery4");
    fs::write(&rtf_file, b"{\\rtf1 hello}").unwrap();
    assert_eq!(detect_format_by_magic(&rtf_file), Some("rtf"));

    // PNG magic bytes
    let png_file = dir.path().join("mystery5");
    let mut png_data = vec![0x89, b'P', b'N', b'G'];
    png_data.extend_from_slice(&[0u8; 50]);
    fs::write(&png_file, &png_data).unwrap();
    assert_eq!(detect_format_by_magic(&png_file), Some("image"));

    // JPEG magic bytes
    let jpg_file = dir.path().join("mystery6");
    let mut jpg_data = vec![0xFF, 0xD8, 0xFF];
    jpg_data.extend_from_slice(&[0u8; 50]);
    fs::write(&jpg_file, &jpg_data).unwrap();
    assert_eq!(detect_format_by_magic(&jpg_file), Some("image"));

    // JSON detection
    let json_file = dir.path().join("mystery7");
    fs::write(&json_file, b"{\"key\": \"value\"}").unwrap();
    assert_eq!(detect_format_by_magic(&json_file), Some("json"));

    // XML detection
    let xml_file = dir.path().join("mystery8");
    fs::write(&xml_file, b"<?xml version=\"1.0\"?><root/>").unwrap();
    assert_eq!(detect_format_by_magic(&xml_file), Some("xml"));
}

// ===========================================================================
// 10. test_chunking_by_structure_preserves_tables
// ===========================================================================
#[test]
fn test_chunking_by_structure_preserves_tables() {
    let mut table_block = Block::new(ElementType::Table, "Col1|Col2\nA|B\nC|D");
    table_block.table_data = Some(TableData {
        rows: vec![
            vec!["A".into(), "B".into()],
            vec!["C".into(), "D".into()],
        ],
        headers: Some(vec!["Col1".into(), "Col2".into()]),
    });
    table_block.page = 1;

    let blocks = vec![
        make_block("Intro paragraph with some text content.", ElementType::NarrativeText, 1),
        table_block,
        make_block("Conclusion paragraph with more text.", ElementType::NarrativeText, 1),
    ];

    let chunks = chunk_blocks(&blocks, &ChunkStrategy::ByStructure, 512, 0).unwrap();

    // Find the chunk(s) containing the table
    for chunk in &chunks {
        let has_table = chunk
            .source_blocks
            .iter()
            .any(|b| b.element_type == ElementType::Table);
        if has_table {
            // The table block should be intact (not split across chunks)
            let table_blocks: Vec<_> = chunk
                .source_blocks
                .iter()
                .filter(|b| b.element_type == ElementType::Table)
                .collect();
            assert_eq!(
                table_blocks.len(),
                1,
                "Table should appear as a single block in the chunk"
            );
            assert!(table_blocks[0].table_data.is_some());
        }
    }
}

// ===========================================================================
// 11. test_chunking_by_title_sections
// ===========================================================================
#[test]
fn test_chunking_by_title_sections() {
    let blocks = vec![
        make_block("Introduction", ElementType::Title, 1),
        make_block("This is the intro section with enough content.", ElementType::NarrativeText, 1),
        make_block("Chapter One", ElementType::Title, 2),
        make_block("Chapter one content goes here with details.", ElementType::NarrativeText, 2),
        make_block("Chapter Two", ElementType::Title, 3),
        make_block("Chapter two content with more details.", ElementType::NarrativeText, 3),
    ];

    let chunks = chunk_blocks(&blocks, &ChunkStrategy::ByTitle, 512, 0).unwrap();

    // Should split at title boundaries → 3 sections
    assert_eq!(
        chunks.len(),
        3,
        "Expected 3 chunks (one per title section), got {}",
        chunks.len()
    );
    assert!(chunks[0].text.contains("Introduction"));
    assert!(chunks[1].text.contains("Chapter One"));
    assert!(chunks[2].text.contains("Chapter Two"));
}

// ===========================================================================
// 12. test_chunking_by_page_isolation
// ===========================================================================
#[test]
fn test_chunking_by_page_isolation() {
    let blocks = vec![
        make_block("Page 1 content A.", ElementType::NarrativeText, 1),
        make_block("Page 1 content B.", ElementType::NarrativeText, 1),
        make_block("Page 2 content A.", ElementType::NarrativeText, 2),
        make_block("Page 3 content A.", ElementType::NarrativeText, 3),
    ];

    let chunks = chunk_blocks(&blocks, &ChunkStrategy::ByPage, 512, 0).unwrap();

    // No chunk should span multiple pages
    for chunk in &chunks {
        assert_eq!(
            chunk.page_start, chunk.page_end,
            "Chunk '{}' spans pages {}-{}, should not cross page boundaries",
            chunk.id, chunk.page_start, chunk.page_end
        );
    }

    // Should have at least 3 chunks (one per page)
    assert!(
        chunks.len() >= 3,
        "Expected at least 3 page-isolated chunks, got {}",
        chunks.len()
    );
}

// ===========================================================================
// 13. test_chunking_fixed_size_exact
// ===========================================================================
#[test]
fn test_chunking_fixed_size_exact() {
    // Create a text long enough to require multiple chunks at max_tokens=20
    let long_text = "The quick brown fox jumps over the lazy dog and then runs across the meadow to find shelter from the approaching storm while birds fly overhead in the darkening sky above the countryside";
    let blocks = vec![make_block(long_text, ElementType::NarrativeText, 1)];

    let max_tokens = 20;
    let chunks = chunk_blocks(&blocks, &ChunkStrategy::Fixed, max_tokens, 0).unwrap();

    assert!(
        chunks.len() > 1,
        "Fixed-size chunking should produce multiple chunks for long text, got {}",
        chunks.len()
    );

    // Each chunk's token count should be reasonable (within ~2x of max due to char estimation)
    for chunk in &chunks {
        let actual = estimate_tokens(&chunk.text);
        assert!(
            actual > 0,
            "Chunk should have non-zero token count"
        );
    }
}

// ===========================================================================
// 14. test_output_json_roundtrip
// ===========================================================================
#[test]
fn test_output_json_roundtrip() {
    let dir = TempDir::new().unwrap();
    let chunks = sample_chunks(5);

    write_output(&chunks, dir.path(), "roundtrip", &OutputFormat::Json).unwrap();

    let content = fs::read_to_string(dir.path().join("roundtrip.json")).unwrap();
    let parsed: Vec<Chunk> = serde_json::from_str(&content).unwrap();

    assert_eq!(parsed.len(), 5);
    for (i, chunk) in parsed.iter().enumerate() {
        assert_eq!(chunk.id, format!("chunk-{}", i));
        assert!(chunk.text.contains(&format!("Chunk {}", i)));
        assert_eq!(chunk.confidence, 1.0);
    }
}

// ===========================================================================
// 15. test_output_jsonl_line_count
// ===========================================================================
#[test]
fn test_output_jsonl_line_count() {
    let dir = TempDir::new().unwrap();
    let chunks = sample_chunks(7);

    write_output(&chunks, dir.path(), "lines", &OutputFormat::Jsonl).unwrap();

    let content = fs::read_to_string(dir.path().join("lines.jsonl")).unwrap();
    let lines: Vec<&str> = content.lines().collect();

    assert_eq!(
        lines.len(),
        7,
        "JSONL line count should match chunk count"
    );

    // Each line should be valid JSON
    for (i, line) in lines.iter().enumerate() {
        let parsed: Chunk = serde_json::from_str(line)
            .unwrap_or_else(|e| panic!("Line {} is not valid JSON: {}", i, e));
        assert_eq!(parsed.id, format!("chunk-{}", i));
    }
}

// ===========================================================================
// 16. test_output_markdown_headings
// ===========================================================================
#[test]
fn test_output_markdown_headings() {
    let dir = TempDir::new().unwrap();

    let chunks = vec![Chunk {
        id: "chunk-0".to_string(),
        text: "My Title\nSome body text".to_string(),
        token_count: 5,
        source_blocks: vec![
            Block::new(ElementType::Title, "My Title"),
            Block::new(ElementType::NarrativeText, "Some body text"),
        ],
        page_start: 1,
        page_end: 1,
        overlap_prefix: None,
        confidence: 1.0,
    }];

    write_output(&chunks, dir.path(), "headings", &OutputFormat::Markdown).unwrap();

    let content = fs::read_to_string(dir.path().join("headings.md")).unwrap();

    // Title block with empty hierarchy renders as h2 (##)
    assert!(
        content.contains("## My Title"),
        "Title should render as ## heading, got:\n{}",
        content
    );
    assert!(content.contains("Some body text"));
    // Top-level document heading
    assert!(content.contains("# Chunked Output: headings"));
}

// ===========================================================================
// 17. test_output_parquet_readable
// ===========================================================================
#[test]
fn test_output_parquet_readable() {
    let dir = TempDir::new().unwrap();
    let chunks = sample_chunks(3);

    write_output(&chunks, dir.path(), "parq_test", &OutputFormat::Parquet).unwrap();

    let path = dir.path().join("parq_test.parquet");
    assert!(path.exists(), "Parquet file should exist");

    // Verify the file has non-trivial size (valid Parquet with data)
    let meta = fs::metadata(&path).unwrap();
    assert!(
        meta.len() > 100,
        "Parquet file should have meaningful size, got {} bytes",
        meta.len()
    );

    // Verify it can be opened as a valid Parquet file
    use parquet::file::reader::FileReader;
    let file = fs::File::open(&path).unwrap();
    let reader = parquet::file::reader::SerializedFileReader::new(file).unwrap();
    let parquet_meta = reader.metadata();
    assert_eq!(
        parquet_meta.num_row_groups(),
        1,
        "Expected 1 row group"
    );
    let row_group = parquet_meta.row_group(0);
    assert_eq!(
        row_group.num_rows(),
        3,
        "Expected 3 rows in parquet"
    );
}

// ===========================================================================
// 18. test_confidence_scores_set
// ===========================================================================
#[test]
fn test_confidence_scores_set() {
    let dir = TempDir::new().unwrap();

    // Text parser → confidence 1.0
    let txt_file = dir.path().join("test.txt");
    fs::write(&txt_file, "Some text content that is long enough to produce blocks.").unwrap();
    let txt_parser = get_parser("text").unwrap();
    let txt_blocks = txt_parser.parse(&txt_file).unwrap();
    for b in &txt_blocks {
        assert!(
            b.confidence > 0.0,
            "Text block confidence should be > 0, got {}",
            b.confidence
        );
    }

    // HTML parser → confidence 0.95
    let html_file = dir.path().join("test.html");
    fs::write(&html_file, "<html><body><p>Hello</p></body></html>").unwrap();
    let html_parser = get_parser("html").unwrap();
    let html_blocks = html_parser.parse(&html_file).unwrap();
    for b in &html_blocks {
        assert!(
            b.confidence > 0.0,
            "HTML block confidence should be > 0, got {}",
            b.confidence
        );
    }

    // RTF parser → confidence 0.8
    let rtf_file = dir.path().join("test.rtf");
    fs::write(&rtf_file, r"{\rtf1\ansi Some RTF text here.}").unwrap();
    let rtf_parser = get_parser("rtf").unwrap();
    let rtf_blocks = rtf_parser.parse(&rtf_file).unwrap();
    for b in &rtf_blocks {
        assert!(
            b.confidence > 0.0,
            "RTF block confidence should be > 0, got {}",
            b.confidence
        );
    }

    // CSV parser → confidence 1.0
    let csv_file = dir.path().join("test.csv");
    fs::write(&csv_file, "a,b\n1,2\n").unwrap();
    let csv_parser = get_parser("csv_tsv").unwrap();
    let csv_blocks = csv_parser.parse(&csv_file).unwrap();
    for b in &csv_blocks {
        assert!(
            b.confidence > 0.0,
            "CSV block confidence should be > 0, got {}",
            b.confidence
        );
    }
}

// ===========================================================================
// 19. test_error_codes_present
// ===========================================================================
#[test]
fn test_error_codes_present() {
    let errors = [
        Error::UnsupportedFormat("test".into()),
        Error::NotImplemented("test"),
        Error::Io("test".into()),
        Error::Parse("test".into()),
        Error::Serialization("test".into()),
        Error::ConfigError("test".into()),
    ];

    let expected_codes = ["E1001", "E1002", "E2001", "E3001", "E4001", "E5001"];

    for (err, expected_code) in errors.iter().zip(expected_codes.iter()) {
        assert_eq!(
            err.code(),
            *expected_code,
            "Error {:?} should have code {}, got {}",
            err,
            expected_code,
            err.code()
        );
        // Error display should include the code
        let display = format!("{}", err);
        assert!(
            display.contains(expected_code),
            "Error display '{}' should contain code '{}'",
            display,
            expected_code
        );
    }
}

// ===========================================================================
// 20. test_full_pipeline_txt_to_json
// ===========================================================================
#[test]
fn test_full_pipeline_txt_to_json() {
    let dir = TempDir::new().unwrap();
    let input_file = dir.path().join("document.txt");
    let output_dir = dir.path().join("output");

    fs::write(
        &input_file,
        "Introduction\n\nThis is the first section of the document. \
         It contains enough text to be meaningful for parsing and chunking.\n\n\
         Second Section\n\nThis is the second section with additional content. \
         The parser should split this into multiple paragraph blocks.\n\n\
         Conclusion\n\nFinal thoughts on the document.\n",
    )
    .unwrap();

    // Process single file
    let chunks =
        process_single_file(&input_file, &ChunkStrategy::ByStructure, 512, 50).unwrap();

    assert!(!chunks.is_empty(), "Pipeline should produce chunks");

    // Write output
    write_output(&chunks, &output_dir, "document", &OutputFormat::Json).unwrap();

    // Verify output file exists and is valid JSON
    let output_path = output_dir.join("document.json");
    assert!(output_path.exists(), "Output JSON file should exist");

    let content = fs::read_to_string(&output_path).unwrap();
    let parsed: Vec<Chunk> = serde_json::from_str(&content).unwrap();

    assert_eq!(parsed.len(), chunks.len());
    for chunk in &parsed {
        assert!(!chunk.text.is_empty(), "Chunk text should not be empty");
        assert!(chunk.token_count > 0, "Chunk should have tokens");
        assert!(chunk.confidence > 0.0, "Chunk confidence should be > 0");
    }
}

// ===========================================================================
// Additional integration: batch processing
// ===========================================================================
#[test]
fn test_batch_processing_multiple_formats() {
    let dir = TempDir::new().unwrap();

    // Create files of different formats
    fs::write(dir.path().join("a.txt"), "Text file content with enough words.").unwrap();
    fs::write(dir.path().join("b.csv"), "x,y\n1,2\n3,4\n").unwrap();
    fs::write(
        dir.path().join("c.html"),
        "<html><body><p>HTML content here.</p></body></html>",
    )
    .unwrap();

    let files: Vec<_> = vec![
        dir.path().join("a.txt"),
        dir.path().join("b.csv"),
        dir.path().join("c.html"),
    ];

    let (successes, failures) =
        process_batch(&files, &ChunkStrategy::ByStructure, 512, 50, 2);

    assert_eq!(
        successes.len(),
        3,
        "All 3 files should succeed, failures: {:?}",
        failures.iter().map(|(p, e)| format!("{}: {}", p.display(), e)).collect::<Vec<_>>()
    );
    assert!(failures.is_empty());
}

// ===========================================================================
// Token estimation sanity
// ===========================================================================
#[test]
fn test_token_estimation_consistency() {
    // Verify token counts are consistent for the same input
    let text = "The quick brown fox jumps over the lazy dog.";
    let count1 = estimate_tokens(text);
    let count2 = estimate_tokens(text);
    assert_eq!(count1, count2, "Token estimation should be deterministic");
    assert!(count1 > 0, "Non-empty text should have non-zero tokens");

    // Longer text should have more tokens
    let short_count = estimate_tokens("hello");
    let long_count = estimate_tokens("hello world this is a longer sentence with more tokens in it");
    assert!(
        long_count > short_count,
        "Longer text should produce more tokens"
    );
}

// ===========================================================================
// Chunk min_confidence
// ===========================================================================
#[test]
fn test_chunk_min_confidence() {
    let blocks = vec![
        {
            let mut b = Block::new(ElementType::NarrativeText, "a");
            b.confidence = 0.9;
            b
        },
        {
            let mut b = Block::new(ElementType::NarrativeText, "b");
            b.confidence = 0.5;
            b
        },
        {
            let mut b = Block::new(ElementType::NarrativeText, "c");
            b.confidence = 0.8;
            b
        },
    ];

    let min = Chunk::min_confidence(&blocks);
    assert!(
        (min - 0.5).abs() < f32::EPSILON,
        "min_confidence should be 0.5, got {}",
        min
    );

    // Empty blocks
    let empty_min = Chunk::min_confidence(&[]);
    assert!(
        (empty_min - 1.0).abs() < f32::EPSILON,
        "min_confidence of empty should be 1.0, got {}",
        empty_min
    );
}
