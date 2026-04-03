#!/bin/bash
# Run all benchmarks and generate comparison report.
#
# Usage:
#   bash benchmarks/run_all.sh
#
# Prerequisites:
#   - cargo build --release (for Parser Chunker)
#   - pip install unstructured (for Unstructured benchmark)
#   - pip install docling (for Docling benchmark)
#   - pip install langchain langchain-text-splitters langchain-community (for LangChain benchmark)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_DIR"

echo "========================================="
echo " Parser Chunker Competitive Benchmark"
echo "========================================="
echo ""
echo "Working directory: $PROJECT_DIR"
echo "Date: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
echo ""

mkdir -p benchmark-results

# --- Step 1: Generate corpus ---
echo "[1/6] Generating benchmark corpus..."
python3 scripts/generate_benchmark_corpus.py --output benchmark-corpus
echo ""

# --- Step 2: Parser Chunker ---
echo "[2/6] Running Parser Chunker benchmark..."
bash benchmarks/bench_parser_chunker.sh 2>&1 | tee benchmark-results/parser-chunker.log
echo ""

# --- Step 3: Unstructured ---
echo "[3/6] Running Unstructured.io benchmark..."
if python3 -c "import unstructured" 2>/dev/null; then
    python3 benchmarks/bench_unstructured.py 2>&1 | tee benchmark-results/unstructured.log
else
    echo "SKIP: unstructured not installed (pip install unstructured)"
fi
echo ""

# --- Step 4: Docling ---
echo "[4/6] Running Docling benchmark..."
if python3 -c "import docling" 2>/dev/null; then
    python3 benchmarks/bench_docling.py 2>&1 | tee benchmark-results/docling.log
else
    echo "SKIP: docling not installed (pip install docling)"
fi
echo ""

# --- Step 5: LangChain ---
echo "[5/6] Running LangChain benchmark..."
if python3 -c "import langchain_text_splitters" 2>/dev/null; then
    python3 benchmarks/bench_langchain.py 2>&1 | tee benchmark-results/langchain.log
else
    echo "SKIP: langchain-text-splitters not installed (pip install langchain langchain-text-splitters)"
fi
echo ""

# --- Step 6: Generate report ---
echo "[6/6] Generating comparison report..."
python3 benchmarks/generate_report.py
echo ""

echo "========================================="
echo " Benchmark Complete"
echo "========================================="
echo ""
echo "Results directory: benchmark-results/"
echo "Report: benchmark-results/BENCHMARK_REPORT.md"
