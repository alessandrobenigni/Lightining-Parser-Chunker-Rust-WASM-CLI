#!/usr/bin/env python3
"""Benchmark Docling on the standard corpus.

Requires: pip install docling

Processes each file with Docling's DocumentConverter and reports
timing metrics in CSV format compatible with the report generator.

Usage:
    python benchmarks/bench_docling.py [--corpus benchmark-corpus]
"""

import argparse
import csv
import os
import sys
import time
from pathlib import Path


def check_import():
    try:
        from docling.document_converter import DocumentConverter  # noqa: F401
        return True
    except ImportError:
        print("ERROR: docling not installed. Run: pip install docling")
        print("Skipping Docling benchmark.")
        return False


def benchmark_directory(input_dir: Path) -> dict:
    """Benchmark all files in a directory. Returns timing stats."""
    from docling.document_converter import DocumentConverter

    files = sorted([f for f in input_dir.iterdir() if f.is_file()])
    if not files:
        return {"files": 0, "chunks": 0, "elapsed_sec": 0, "files_per_sec": 0}

    converter = DocumentConverter()
    total_chunks = 0
    processed = 0
    errors = 0

    start = time.perf_counter()

    for filepath in files:
        try:
            result = converter.convert(str(filepath))
            # Count document elements/chunks
            doc = result.document
            # Docling structures documents as a tree; count text items
            chunks = 0
            if hasattr(doc, "texts"):
                chunks = len(list(doc.texts))
            elif hasattr(doc, "export_to_dict"):
                d = doc.export_to_dict()
                chunks = len(d.get("texts", d.get("elements", [])))
            else:
                chunks = 1  # At minimum, the document itself
            total_chunks += chunks
            processed += 1
        except Exception as e:
            errors += 1
            print(f"  FAIL: {filepath.name}: {e}", file=sys.stderr)

    elapsed = time.perf_counter() - start
    fps = processed / elapsed if elapsed > 0 else 0

    return {
        "files": processed,
        "chunks": total_chunks,
        "elapsed_sec": round(elapsed, 3),
        "files_per_sec": round(fps, 1),
        "errors": errors,
    }


def main():
    parser = argparse.ArgumentParser(description="Benchmark Docling")
    parser.add_argument("--corpus", default="benchmark-corpus", help="Corpus directory")
    args = parser.parse_args()

    if not check_import():
        sys.exit(1)

    corpus = Path(args.corpus)
    if not corpus.exists():
        print(f"ERROR: Corpus not found at {corpus}")
        sys.exit(1)

    results_dir = Path("benchmark-results")
    results_dir.mkdir(exist_ok=True)
    results_file = results_dir / "docling-timings.csv"

    print("=== Docling Benchmark ===")
    print(f"Corpus: {corpus}")
    print(f"Date:   {time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime())}")
    print()

    categories = ["small", "medium", "large", "mixed-format"]

    with open(results_file, "w", newline="") as csvfile:
        writer = csv.writer(csvfile)
        writer.writerow(["category", "files", "chunks", "elapsed_sec", "files_per_sec"])

        for cat in categories:
            input_dir = corpus / cat
            if not input_dir.exists():
                print(f"SKIP: {input_dir}")
                continue

            file_count = len([f for f in input_dir.iterdir() if f.is_file()])
            print(f"--- {cat} ({file_count} files) ---")

            stats = benchmark_directory(input_dir)
            writer.writerow([
                cat,
                stats["files"],
                stats["chunks"],
                stats["elapsed_sec"],
                stats["files_per_sec"],
            ])

            print(f"  -> {stats['files']} files, {stats['chunks']} chunks "
                  f"in {stats['elapsed_sec']}s ({stats['files_per_sec']} files/sec)")
            if stats.get("errors", 0) > 0:
                print(f"  -> {stats['errors']} errors")
            print()

    print("=== Docling Benchmark Complete ===")
    print(f"Results: {results_file}")


if __name__ == "__main__":
    main()
