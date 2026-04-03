# Changelog

All notable changes to Lightning Parser Chunker will be documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

## [0.1.0] - 2026-04-03

### Added
- 14 document format parsers (PDF, DOCX, XLSX, PPTX, HTML, EML, MSG, CSV, TXT, Markdown, RTF, XML, JSON, Images)
- 4 chunking strategies (by-structure, by-title, by-page, fixed-size)
- 5 output formats (JSON, JSONL, Parquet, Markdown, Unstructured-compatible)
- PDFium-powered PDF engine (Chrome-grade quality)
- PaddleOCR ONNX pipeline for scanned documents
- OTSL table structure decoder (validated against TableFormer)
- Real BPE token counting (cl100k_base via bpe-openai)
- Per-element confidence scores
- Debug/inspection mode with 4 pipeline stages
- Structured error codes (E1001-E5001)
- Shell completions (bash, zsh, fish, PowerShell)
- TOML configuration file support
- Per-document config overrides via sidecar files
- @argfile support for CI pipelines
- rayon parallel batch processing (--workers N)
- Partial failure handling with exit codes
- Memory-mapped I/O for large files
- GitHub Actions CI for 5 platform targets
- Competitive benchmark suite vs Unstructured, Docling, LangChain
- 189 tests with 51 accuracy-specific tests
