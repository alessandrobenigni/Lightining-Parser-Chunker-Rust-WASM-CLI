# Contributing to Lightning Parser Chunker

Thank you for your interest in contributing. Parser Chunker is a performance-critical project — every change must maintain or improve throughput, correctness, and binary size.

## Contributor License Agreement (CLA)

Before your first contribution can be merged, you must agree to our [Contributor License Agreement](CLA.md). This is required because Lightning Parser Chunker uses a dual-license model (AGPL-3.0 + commercial), and we need the legal right to offer commercial licenses.

**How to sign:** Add `Signed-off-by` to your commits using `git commit -s`. This indicates you've read and agreed to the CLA.

All commits must include this line — the DCO bot will block PRs without it.

## Getting Started

1. Read the [CLA](CLA.md)
2. Fork the repository
3. Clone your fork: `git clone https://github.com/YOUR-USERNAME/Lightining-Parser-Chunker-Rust-WASM-CLI.git`
4. Create a branch: `git checkout -b feature/your-feature`
5. Make changes and add tests
6. Run the full test suite: `cargo test --all`
7. Run clippy: `cargo clippy --all-targets`
8. Commit with sign-off: `git commit -s -m "your message"`
9. Submit a pull request

## Code Standards

- **All code must pass `cargo clippy` with zero warnings**
- **All code must pass `cargo fmt --check`**
- **All new features require tests**
- **Benchmark any performance-sensitive changes with `cargo bench`**
- **No new runtime dependencies without discussion** — binary size matters

## Architecture Overview

The pipeline flows through five stages:

```
Input → Format Detection → Parsing → Chunking → Output Serialization
```

Each format parser implements the `FormatParser` trait. Chunking strategies implement the `ChunkStrategy` trait. Output serializers implement the `OutputWriter` trait.

Key modules:

| Module | Purpose |
|--------|---------|
| `src/format/` | Per-format parsers (PDF, DOCX, HTML, etc.) |
| `src/chunk/` | Chunking strategies (by-structure, by-title, etc.) |
| `src/output/` | Output serializers (JSON, JSONL, Parquet, etc.) |
| `src/pipeline/` | Orchestration, parallel execution, error handling |
| `src/detect/` | Format detection (extension + magic bytes) |
| `src/token/` | BPE tokenizer (cl100k_base) |

## Adding a New Format Parser

1. Create `src/format/your_format.rs`
2. Implement the `FormatParser` trait
3. Add to `src/format/mod.rs` (module declaration + `get_parser` routing)
4. Add format detection in `detect_format()` and `detect_format_by_magic()`
5. Write tests in `tests/format_tests.rs`
6. Add test fixtures to `test-fixtures/`
7. Update the format table in `README.md`

## Adding a New Chunking Strategy

1. Create `src/chunk/your_strategy.rs`
2. Implement the `ChunkStrategy` trait
3. Add to `src/chunk/mod.rs`
4. Add CLI flag in `src/cli.rs`
5. Write tests and benchmarks

## Pull Request Guidelines

- Keep PRs focused — one feature or fix per PR
- Include before/after benchmark numbers for performance changes
- Update documentation if you change CLI flags or behavior
- Add a changelog entry under `[Unreleased]` in `CHANGELOG.md`

## Running Benchmarks

```bash
# Micro-benchmarks
cargo bench

# Full competitive benchmarks (requires Python competitors installed)
cd benchmark-results
bash run_all.sh
```

## License

By contributing, you agree to the terms of the [Contributor License Agreement](CLA.md). Your contributions will be licensed under the AGPL-3.0, and you grant Alessandro Benigni the right to sublicense under commercial terms as described in the CLA.
