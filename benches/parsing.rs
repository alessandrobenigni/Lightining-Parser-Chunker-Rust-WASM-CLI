//! Comprehensive Criterion benchmarks for Parser Chunker.
//!
//! Layers benchmarked:
//! 1. Format detection
//! 2. Parsing (per format)
//! 3. Chunking (per strategy)
//! 4. Token counting
//! 5. Output serialization
//! 6. Full pipeline
//! 7. Batch scaling

use std::fs;
use std::path::PathBuf;

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};

use parser_chunker::chunking::{chunk_blocks, estimate_tokens};
use parser_chunker::cli::{ChunkStrategy, OutputFormat};
use parser_chunker::format::{detect_format, get_parser};
use parser_chunker::model::{Block, Chunk, ElementType};
use parser_chunker::orchestrator::{process_batch, process_single_file};
use parser_chunker::output::write_output;

// ---------------------------------------------------------------------------
// Data generators
// ---------------------------------------------------------------------------

/// Generate a realistic ~1KB text string.
fn generate_text_1kb() -> String {
    let sentence = "The quick brown fox jumps over the lazy dog near the riverbank. ";
    sentence.repeat(16) // ~1024 chars
}

/// Generate N blocks of ~50 words each.
fn generate_blocks(count: usize) -> Vec<Block> {
    (0..count)
        .map(|i| {
            let words: String = (0..50)
                .map(|w| format!("word{}x{}", i, w))
                .collect::<Vec<_>>()
                .join(" ");
            let mut b = Block::new(ElementType::NarrativeText, words);
            b.page = (i as u32 / 10) + 1;
            b
        })
        .collect()
}

/// Generate blocks with title markers for by_title chunking.
fn generate_titled_blocks(count: usize) -> Vec<Block> {
    let mut blocks = Vec::with_capacity(count);
    for i in 0..count {
        if i % 10 == 0 {
            let mut b = Block::new(ElementType::Title, format!("Section {}", i / 10));
            b.page = (i as u32 / 10) + 1;
            blocks.push(b);
        } else {
            let words: String = (0..50)
                .map(|w| format!("word{}x{}", i, w))
                .collect::<Vec<_>>()
                .join(" ");
            let mut b = Block::new(ElementType::NarrativeText, words);
            b.page = (i as u32 / 10) + 1;
            blocks.push(b);
        }
    }
    blocks
}

/// Generate blocks spread across distinct pages for by_page chunking.
fn generate_paged_blocks(count: usize) -> Vec<Block> {
    (0..count)
        .map(|i| {
            let words: String = (0..50)
                .map(|w| format!("word{}x{}", i, w))
                .collect::<Vec<_>>()
                .join(" ");
            let mut b = Block::new(ElementType::NarrativeText, words);
            b.page = i as u32 + 1; // each block on its own page
            b
        })
        .collect()
}

/// Generate sample chunks for output benchmarks.
fn generate_chunks(count: usize) -> Vec<Chunk> {
    (0..count)
        .map(|i| Chunk {
            id: format!("chunk-{}", i),
            text: format!(
                "This is chunk number {} with enough text to be realistic for benchmarking output serialization performance.",
                i
            ),
            token_count: 20,
            source_blocks: vec![Block::new(ElementType::NarrativeText, "source text")],
            page_start: 1,
            page_end: 1,
            overlap_prefix: if i > 0 {
                Some("overlap text from previous chunk".to_string())
            } else {
                None
            },
            confidence: 1.0,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Layer 1: Format Detection
// ---------------------------------------------------------------------------

fn bench_format_detection(c: &mut Criterion) {
    let extensions = vec![
        "report.pdf",
        "doc.docx",
        "data.xlsx",
        "page.html",
        "data.csv",
        "readme.txt",
        "readme.md",
        "style.rtf",
        "data.xml",
        "data.json",
        "unknown.xyz",
    ];

    let mut group = c.benchmark_group("format_detection");
    for ext in &extensions {
        group.bench_with_input(BenchmarkId::new("detect", ext), ext, |b, path| {
            let p = std::path::Path::new(path);
            b.iter(|| black_box(detect_format(p)));
        });
    }
    group.finish();
}

// ---------------------------------------------------------------------------
// Layer 2: Parsing (per format)
// ---------------------------------------------------------------------------

fn bench_parse_text(c: &mut Criterion) {
    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join("bench.txt");
    fs::write(&file, generate_text_1kb()).unwrap();

    let parser = get_parser("text").unwrap();
    c.bench_function("parse_text_1kb", |b| {
        b.iter(|| black_box(parser.parse(&file).unwrap()));
    });
}

fn bench_parse_csv(c: &mut Criterion) {
    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join("bench.csv");
    let mut csv_data = String::from("col_a,col_b,col_c,col_d\n");
    for i in 0..100 {
        csv_data.push_str(&format!("val_{}_a,val_{}_b,val_{}_c,val_{}_d\n", i, i, i, i));
    }
    fs::write(&file, &csv_data).unwrap();

    let parser = get_parser("csv_tsv").unwrap();
    c.bench_function("parse_csv_100rows", |b| {
        b.iter(|| black_box(parser.parse(&file).unwrap()));
    });
}

fn bench_parse_html(c: &mut Criterion) {
    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join("bench.html");
    let mut html = String::from("<!DOCTYPE html><html><body>");
    for i in 0..20 {
        html.push_str(&format!("<h2>Section {}</h2>", i));
        html.push_str(&format!(
            "<p>Paragraph {} with some text content for benchmarking.</p>",
            i
        ));
    }
    html.push_str("<table><tr><th>A</th><th>B</th></tr>");
    for i in 0..10 {
        html.push_str(&format!("<tr><td>{}</td><td>{}</td></tr>", i, i * 2));
    }
    html.push_str("</table></body></html>");
    fs::write(&file, &html).unwrap();

    let parser = get_parser("html").unwrap();
    c.bench_function("parse_html_20sections", |b| {
        b.iter(|| black_box(parser.parse(&file).unwrap()));
    });
}

fn bench_parse_markdown(c: &mut Criterion) {
    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join("bench.md");
    let mut md = String::new();
    for i in 0..20 {
        md.push_str(&format!("## Section {}\n\n", i));
        md.push_str(&format!(
            "This is section {} with enough content to be realistic for benchmarking.\n\n",
            i
        ));
        md.push_str("- Item one\n- Item two\n\n");
        if i % 5 == 0 {
            md.push_str("```\ncode block content\n```\n\n");
        }
    }
    fs::write(&file, &md).unwrap();

    let parser = get_parser("text").unwrap();
    c.bench_function("parse_markdown_20sections", |b| {
        b.iter(|| black_box(parser.parse(&file).unwrap()));
    });
}

fn bench_parse_rtf(c: &mut Criterion) {
    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join("bench.rtf");
    let mut rtf = String::from(r"{\rtf1\ansi ");
    for i in 0..20 {
        rtf.push_str(&format!(
            "Paragraph {} with some content for benchmarking RTF parsing performance. \\par ",
            i
        ));
    }
    rtf.push('}');
    fs::write(&file, &rtf).unwrap();

    let parser = get_parser("rtf").unwrap();
    c.bench_function("parse_rtf_20para", |b| {
        b.iter(|| black_box(parser.parse(&file).unwrap()));
    });
}

// ---------------------------------------------------------------------------
// Layer 3: Chunking (per strategy)
// ---------------------------------------------------------------------------

fn bench_chunk_by_structure(c: &mut Criterion) {
    let blocks = generate_blocks(100);
    c.bench_function("chunk_by_structure_100blocks", |b| {
        b.iter(|| black_box(chunk_blocks(&blocks, &ChunkStrategy::ByStructure, 512, 50).unwrap()));
    });
}

fn bench_chunk_by_title(c: &mut Criterion) {
    let blocks = generate_titled_blocks(100);
    c.bench_function("chunk_by_title_100blocks", |b| {
        b.iter(|| black_box(chunk_blocks(&blocks, &ChunkStrategy::ByTitle, 512, 50).unwrap()));
    });
}

fn bench_chunk_by_page(c: &mut Criterion) {
    let blocks = generate_paged_blocks(100);
    c.bench_function("chunk_by_page_100blocks", |b| {
        b.iter(|| black_box(chunk_blocks(&blocks, &ChunkStrategy::ByPage, 512, 50).unwrap()));
    });
}

fn bench_chunk_fixed_size(c: &mut Criterion) {
    let blocks = generate_blocks(100);
    c.bench_function("chunk_fixed_100blocks", |b| {
        b.iter(|| black_box(chunk_blocks(&blocks, &ChunkStrategy::Fixed, 512, 50).unwrap()));
    });
}

// ---------------------------------------------------------------------------
// Layer 4: Token Counting
// ---------------------------------------------------------------------------

fn bench_token_counting(c: &mut Criterion) {
    let mut group = c.benchmark_group("token_counting");

    let s100 = "a".repeat(100);
    let s1000 = "The quick brown fox jumps. ".repeat(40);

    group.bench_function("10_chars", |b| {
        b.iter(|| black_box(estimate_tokens("Hello wor!")));
    });
    group.bench_function("100_chars", |b| {
        b.iter(|| black_box(estimate_tokens(&s100)));
    });
    group.bench_function("1000_chars", |b| {
        b.iter(|| black_box(estimate_tokens(&s1000)));
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// Layer 5: Output Serialization
// ---------------------------------------------------------------------------

fn bench_output_json(c: &mut Criterion) {
    let chunks = generate_chunks(100);
    let dir = tempfile::TempDir::new().unwrap();

    c.bench_function("output_json_100chunks", |b| {
        b.iter(|| {
            write_output(
                black_box(&chunks),
                dir.path(),
                "bench_json",
                &OutputFormat::Json,
            )
            .unwrap();
        });
    });
}

fn bench_output_jsonl(c: &mut Criterion) {
    let chunks = generate_chunks(100);
    let dir = tempfile::TempDir::new().unwrap();

    c.bench_function("output_jsonl_100chunks", |b| {
        b.iter(|| {
            write_output(
                black_box(&chunks),
                dir.path(),
                "bench_jsonl",
                &OutputFormat::Jsonl,
            )
            .unwrap();
        });
    });
}

fn bench_output_markdown(c: &mut Criterion) {
    let chunks = generate_chunks(100);
    let dir = tempfile::TempDir::new().unwrap();

    c.bench_function("output_markdown_100chunks", |b| {
        b.iter(|| {
            write_output(
                black_box(&chunks),
                dir.path(),
                "bench_md",
                &OutputFormat::Markdown,
            )
            .unwrap();
        });
    });
}

fn bench_output_parquet(c: &mut Criterion) {
    let chunks = generate_chunks(100);
    let dir = tempfile::TempDir::new().unwrap();

    c.bench_function("output_parquet_100chunks", |b| {
        b.iter(|| {
            write_output(
                black_box(&chunks),
                dir.path(),
                "bench_parquet",
                &OutputFormat::Parquet,
            )
            .unwrap();
        });
    });
}

// ---------------------------------------------------------------------------
// Layer 6: Full Pipeline
// ---------------------------------------------------------------------------

fn bench_full_pipeline(c: &mut Criterion) {
    let dir = tempfile::TempDir::new().unwrap();

    // Create a realistic text file
    let file = dir.path().join("pipeline.txt");
    let mut content = String::new();
    for i in 0..50 {
        content.push_str(&format!(
            "Section {} begins here.\n\n\
             This is paragraph content for section {}. It contains enough words \
             to be a realistic document that exercises the full pipeline from \
             parsing through chunking to output serialization.\n\n",
            i, i
        ));
    }
    fs::write(&file, &content).unwrap();

    let output_dir = dir.path().join("output");
    fs::create_dir_all(&output_dir).unwrap();

    c.bench_function("full_pipeline_50sections", |b| {
        b.iter(|| {
            let chunks = process_single_file(
                black_box(&file),
                &ChunkStrategy::ByStructure,
                512,
                50,
            )
            .unwrap();
            write_output(&chunks, &output_dir, "pipeline", &OutputFormat::Json).unwrap();
            black_box(chunks.len())
        });
    });
}

// ---------------------------------------------------------------------------
// Layer 7: Batch Scaling
// ---------------------------------------------------------------------------

fn bench_batch_scaling(c: &mut Criterion) {
    let dir = tempfile::TempDir::new().unwrap();
    let input_dir = dir.path().join("inputs");
    fs::create_dir_all(&input_dir).unwrap();

    // Create 20 small text files (enough to see scaling without being too slow)
    let mut files: Vec<PathBuf> = Vec::new();
    for i in 0..20 {
        let file = input_dir.join(format!("file_{}.txt", i));
        let content = format!(
            "Document {} content.\n\nParagraph one of document {}. \
             It has enough text to create at least one chunk.\n\n\
             Paragraph two of document {}.\n",
            i, i, i
        );
        fs::write(&file, &content).unwrap();
        files.push(file);
    }

    let mut group = c.benchmark_group("batch_scaling");
    // Limit sample size since batch processing is slower
    group.sample_size(10);

    for workers in [1, 2, 4] {
        group.bench_with_input(
            BenchmarkId::new("workers", workers),
            &workers,
            |b, &w| {
                b.iter(|| {
                    let (successes, failures) = process_batch(
                        black_box(&files),
                        &ChunkStrategy::ByStructure,
                        512,
                        50,
                        w,
                    );
                    black_box((successes.len(), failures.len()))
                });
            },
        );
    }
    group.finish();
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

criterion_group!(
    format_detection,
    bench_format_detection
);

criterion_group!(
    parsing,
    bench_parse_text,
    bench_parse_csv,
    bench_parse_html,
    bench_parse_markdown,
    bench_parse_rtf
);

criterion_group!(
    chunking,
    bench_chunk_by_structure,
    bench_chunk_by_title,
    bench_chunk_by_page,
    bench_chunk_fixed_size
);

criterion_group!(
    token_counting,
    bench_token_counting
);

criterion_group!(
    output,
    bench_output_json,
    bench_output_jsonl,
    bench_output_markdown,
    bench_output_parquet
);

criterion_group!(
    pipeline,
    bench_full_pipeline,
    bench_batch_scaling
);

criterion_main!(
    format_detection,
    parsing,
    chunking,
    token_counting,
    output,
    pipeline
);
