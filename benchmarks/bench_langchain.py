#!/usr/bin/env python3
"""Benchmark LangChain text splitters on the standard corpus.

Requires: pip install langchain langchain-text-splitters

Processes each file with LangChain's document loaders and text splitters,
then reports timing metrics in CSV format compatible with the report generator.

Usage:
    python benchmarks/bench_langchain.py [--corpus benchmark-corpus]
"""

import argparse
import csv
import sys
import time
from pathlib import Path


def check_imports():
    try:
        from langchain_text_splitters import RecursiveCharacterTextSplitter  # noqa: F401
        return True
    except ImportError:
        print("ERROR: langchain-text-splitters not installed.")
        print("Run: pip install langchain langchain-text-splitters langchain-community")
        print("Skipping LangChain benchmark.")
        return False


def load_and_split_file(filepath: Path, splitter) -> int:
    """Load a file and split it. Returns chunk count."""
    suffix = filepath.suffix.lower()
    text = filepath.read_text(encoding="utf-8", errors="replace")

    if suffix == ".csv":
        # For CSV, split on rows then chunk
        chunks = splitter.split_text(text)
    elif suffix == ".html":
        # Use the text splitter on raw HTML (LangChain's HTML loader
        # would need additional deps; this is fair for throughput comparison)
        try:
            from langchain_text_splitters import HTMLHeaderTextSplitter
            headers_to_split_on = [
                ("h1", "Header 1"),
                ("h2", "Header 2"),
                ("h3", "Header 3"),
            ]
            html_splitter = HTMLHeaderTextSplitter(headers_to_split_on=headers_to_split_on)
            html_docs = html_splitter.split_text(text)
            # Further split with recursive splitter
            chunks = splitter.split_documents(html_docs)
        except Exception:
            # Fallback to plain text splitting
            chunks = splitter.split_text(text)
    elif suffix == ".md":
        try:
            from langchain_text_splitters import MarkdownHeaderTextSplitter
            headers_to_split_on = [
                ("#", "Header 1"),
                ("##", "Header 2"),
                ("###", "Header 3"),
            ]
            md_splitter = MarkdownHeaderTextSplitter(headers_to_split_on=headers_to_split_on)
            md_docs = md_splitter.split_text(text)
            chunks = splitter.split_documents(md_docs)
        except Exception:
            chunks = splitter.split_text(text)
    else:
        chunks = splitter.split_text(text)

    return len(chunks) if isinstance(chunks, list) else 0


def benchmark_directory(input_dir: Path) -> dict:
    """Benchmark all files in a directory. Returns timing stats."""
    from langchain_text_splitters import RecursiveCharacterTextSplitter

    splitter = RecursiveCharacterTextSplitter(
        chunk_size=1000,  # ~512 tokens worth of chars
        chunk_overlap=100,
        length_function=len,
    )

    files = sorted([f for f in input_dir.iterdir() if f.is_file()])
    if not files:
        return {"files": 0, "chunks": 0, "elapsed_sec": 0, "files_per_sec": 0}

    total_chunks = 0
    processed = 0
    errors = 0

    start = time.perf_counter()

    for filepath in files:
        try:
            chunk_count = load_and_split_file(filepath, splitter)
            total_chunks += chunk_count
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
    parser = argparse.ArgumentParser(description="Benchmark LangChain text splitters")
    parser.add_argument("--corpus", default="benchmark-corpus", help="Corpus directory")
    args = parser.parse_args()

    if not check_imports():
        sys.exit(1)

    corpus = Path(args.corpus)
    if not corpus.exists():
        print(f"ERROR: Corpus not found at {corpus}")
        sys.exit(1)

    results_dir = Path("benchmark-results")
    results_dir.mkdir(exist_ok=True)
    results_file = results_dir / "langchain-timings.csv"

    print("=== LangChain Benchmark ===")
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

    print("=== LangChain Benchmark Complete ===")
    print(f"Results: {results_file}")


if __name__ == "__main__":
    main()
