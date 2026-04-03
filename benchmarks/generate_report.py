#!/usr/bin/env python3
"""Generate a markdown comparison report from benchmark timing CSVs.

Reads all *-timings.csv files from benchmark-results/ and produces
a formatted markdown table comparing all tools.

Usage:
    python benchmarks/generate_report.py [--results-dir benchmark-results]
"""

import argparse
import csv
import os
import sys
import time
from pathlib import Path


TOOL_DISPLAY_NAMES = {
    "parser-chunker": "Parser Chunker (Rust)",
    "unstructured": "Unstructured.io",
    "docling": "Docling",
    "langchain": "LangChain",
}

CATEGORIES = ["small", "medium", "large", "mixed-format"]


def load_timings(csv_path: Path) -> dict:
    """Load a timings CSV into {category: {files, chunks, elapsed_sec, files_per_sec}}."""
    data = {}
    with open(csv_path, newline="") as f:
        reader = csv.DictReader(f)
        for row in reader:
            cat = row["category"]
            data[cat] = {
                "files": int(row["files"]),
                "chunks": int(row["chunks"]),
                "elapsed_sec": float(row["elapsed_sec"]),
                "files_per_sec": float(row["files_per_sec"]),
            }
    return data


def format_fps(fps: float) -> str:
    if fps == 0:
        return "N/A"
    if fps >= 1000:
        return f"{fps:,.0f}"
    if fps >= 100:
        return f"{fps:.0f}"
    if fps >= 10:
        return f"{fps:.1f}"
    return f"{fps:.2f}"


def format_time(sec: float) -> str:
    if sec == 0:
        return "N/A"
    if sec < 0.01:
        return f"{sec*1000:.1f}ms"
    if sec < 1:
        return f"{sec:.3f}s"
    if sec < 60:
        return f"{sec:.2f}s"
    return f"{sec/60:.1f}m"


def main():
    parser = argparse.ArgumentParser(description="Generate benchmark comparison report")
    parser.add_argument("--results-dir", default="benchmark-results", help="Results directory")
    parser.add_argument("--output", default=None, help="Output file (default: stdout + results dir)")
    args = parser.parse_args()

    results_dir = Path(args.results_dir)
    if not results_dir.exists():
        print(f"ERROR: Results directory not found: {results_dir}")
        sys.exit(1)

    # Discover all timing CSVs
    tools = {}
    for csv_file in sorted(results_dir.glob("*-timings.csv")):
        tool_key = csv_file.stem.replace("-timings", "")
        display_name = TOOL_DISPLAY_NAMES.get(tool_key, tool_key)
        tools[display_name] = load_timings(csv_file)

    if not tools:
        print("ERROR: No timing CSV files found in", results_dir)
        sys.exit(1)

    # Build report
    lines = []
    lines.append("# Parser Benchmark Comparison Report")
    lines.append("")
    lines.append(f"Generated: {time.strftime('%Y-%m-%d %H:%M:%S UTC', time.gmtime())}")
    lines.append("")

    # --- Throughput Table (files/sec) ---
    lines.append("## Throughput (files/sec)")
    lines.append("")

    header = "| Tool | " + " | ".join(CATEGORIES) + " |"
    sep = "|------|" + "|".join("---:" for _ in CATEGORIES) + "|"
    lines.append(header)
    lines.append(sep)

    for tool_name, data in tools.items():
        cells = []
        for cat in CATEGORIES:
            if cat in data:
                fps = data[cat]["files_per_sec"]
                cells.append(f" **{format_fps(fps)}** " if tool_name.startswith("Parser") else f" {format_fps(fps)} ")
            else:
                cells.append(" - ")
        lines.append(f"| {tool_name} |{'|'.join(cells)}|")

    lines.append("")

    # --- Wall-clock Time Table ---
    lines.append("## Wall-clock Time")
    lines.append("")

    header = "| Tool | " + " | ".join(CATEGORIES) + " |"
    lines.append(header)
    lines.append(sep)

    for tool_name, data in tools.items():
        cells = []
        for cat in CATEGORIES:
            if cat in data:
                cells.append(f" {format_time(data[cat]['elapsed_sec'])} ")
            else:
                cells.append(" - ")
        lines.append(f"| {tool_name} |{'|'.join(cells)}|")

    lines.append("")

    # --- Chunks Produced ---
    lines.append("## Chunks Produced")
    lines.append("")

    header = "| Tool | " + " | ".join(CATEGORIES) + " | Total |"
    sep2 = "|------|" + "|".join("---:" for _ in CATEGORIES) + "|---:|"
    lines.append(header)
    lines.append(sep2)

    for tool_name, data in tools.items():
        cells = []
        total = 0
        for cat in CATEGORIES:
            if cat in data:
                c = data[cat]["chunks"]
                cells.append(f" {c:,} ")
                total += c
            else:
                cells.append(" - ")
        cells.append(f" {total:,} ")
        lines.append(f"| {tool_name} |{'|'.join(cells)}|")

    lines.append("")

    # --- Speedup Table ---
    # Find Parser Chunker data for speedup calculation
    pc_data = tools.get("Parser Chunker (Rust)")
    if pc_data and len(tools) > 1:
        lines.append("## Speedup vs Competitors")
        lines.append("")
        lines.append("How many times faster Parser Chunker is compared to each competitor.")
        lines.append("")

        header = "| Competitor | " + " | ".join(CATEGORIES) + " |"
        lines.append(header)
        lines.append(sep)

        for tool_name, data in tools.items():
            if tool_name == "Parser Chunker (Rust)":
                continue
            cells = []
            for cat in CATEGORIES:
                if cat in data and cat in pc_data:
                    pc_fps = pc_data[cat]["files_per_sec"]
                    comp_fps = data[cat]["files_per_sec"]
                    if comp_fps > 0:
                        speedup = pc_fps / comp_fps
                        cells.append(f" **{speedup:.1f}x** ")
                    else:
                        cells.append(" N/A ")
                else:
                    cells.append(" - ")
            lines.append(f"| {tool_name} |{'|'.join(cells)}|")

        lines.append("")

    # --- Summary ---
    lines.append("## Summary")
    lines.append("")
    if pc_data:
        for cat in CATEGORIES:
            if cat in pc_data:
                lines.append(f"- **{cat}**: {format_fps(pc_data[cat]['files_per_sec'])} files/sec "
                             f"({pc_data[cat]['files']} files, {pc_data[cat]['chunks']:,} chunks "
                             f"in {format_time(pc_data[cat]['elapsed_sec'])})")
    lines.append("")
    lines.append("---")
    lines.append("*Benchmark corpus generated with `scripts/generate_benchmark_corpus.py` (seed=42)*")

    report = "\n".join(lines) + "\n"

    # Write to file and stdout
    output_path = Path(args.output) if args.output else results_dir / "BENCHMARK_REPORT.md"
    output_path.write_text(report, encoding="utf-8")
    print(report)
    print(f"\nReport written to: {output_path}")


if __name__ == "__main__":
    main()
