#!/bin/bash
# Benchmark Parser Chunker on the standard corpus.
#
# Outputs structured timing data to stdout for the report generator.
# Usage: bash benchmarks/bench_parser_chunker.sh [corpus_dir] [binary_path]

set -euo pipefail

CORPUS="${1:-benchmark-corpus}"
BINARY="${2:-./target/release/parser-chunker}"
OUTPUT="benchmark-results/parser-chunker"

# Detect .exe on Windows
if [[ ! -f "$BINARY" ]] && [[ -f "${BINARY}.exe" ]]; then
    BINARY="${BINARY}.exe"
fi

if [[ ! -f "$BINARY" ]]; then
    echo "ERROR: Binary not found at $BINARY"
    echo "Run: cargo build --release"
    exit 1
fi

if [[ ! -d "$CORPUS" ]]; then
    echo "ERROR: Corpus not found at $CORPUS"
    echo "Run: python3 scripts/generate_benchmark_corpus.py"
    exit 1
fi

mkdir -p "$OUTPUT"
mkdir -p benchmark-results

echo "=== Parser Chunker Benchmark ==="
echo "Binary: $BINARY"
echo "Corpus: $CORPUS"
echo "Date:   $(date -u +%Y-%m-%dT%H:%M:%SZ)"
echo ""

RESULTS_FILE="benchmark-results/parser-chunker-timings.csv"
echo "category,files,chunks,elapsed_sec,files_per_sec" > "$RESULTS_FILE"

for dir in small medium large mixed-format; do
    input_dir="$CORPUS/$dir"
    output_dir="$OUTPUT/$dir"

    if [[ ! -d "$input_dir" ]]; then
        echo "SKIP: $input_dir not found"
        continue
    fi

    file_count=$(find "$input_dir" -maxdepth 1 -type f | wc -l | tr -d ' ')
    echo "--- $dir ($file_count files) ---"

    rm -rf "$output_dir"
    mkdir -p "$output_dir"

    # Capture stderr (where parser-chunker writes its stats) and time
    START_TIME=$(date +%s%N 2>/dev/null || python3 -c "import time; print(int(time.time()*1e9))")
    STDERR_OUT=$("$BINARY" --input "$input_dir" --output "$output_dir" --format json --log-level error 2>&1 || true)
    END_TIME=$(date +%s%N 2>/dev/null || python3 -c "import time; print(int(time.time()*1e9))")

    echo "$STDERR_OUT"

    # Parse results from parser-chunker output using python (portable, no grep -P)
    read PROCESSED CHUNKS ELAPSED THROUGHPUT < <(python3 -c "
import re, sys
text = '''$STDERR_OUT'''
processed = re.search(r'Processed: (\d+)', text)
chunks = re.search(r'\((\d+) chunks\)', text)
elapsed = re.search(r'in (\d+\.\d+)s', text)
throughput = re.search(r'Throughput: ([\d.]+)', text)
p = processed.group(1) if processed else '0'
c = chunks.group(1) if chunks else '0'
e = elapsed.group(1) if elapsed else '0'
t = throughput.group(1) if throughput else '0'
print(f'{p} {c} {e} {t}')
")

    # Fallback: compute from wall-clock if parser output not captured
    if [[ "$ELAPSED" == "0" ]]; then
        ELAPSED_NS=$((END_TIME - START_TIME))
        ELAPSED=$(python3 -c "print(f'{$ELAPSED_NS / 1e9:.3f}')")
    fi

    if [[ "$THROUGHPUT" == "0" ]] && [[ "$ELAPSED" != "0" ]]; then
        THROUGHPUT=$(python3 -c "e=$ELAPSED; print(f'{int($PROCESSED) / e:.1f}' if e > 0 else '0')")
    fi

    echo "$dir,$PROCESSED,$CHUNKS,$ELAPSED,$THROUGHPUT" >> "$RESULTS_FILE"
    echo "  -> $PROCESSED files, $CHUNKS chunks in ${ELAPSED}s ($THROUGHPUT files/sec)"
    echo ""
done

echo "=== Parser Chunker Benchmark Complete ==="
echo "Results: $RESULTS_FILE"
