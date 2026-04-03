#!/usr/bin/env bash
set -euo pipefail

# Usage: package.sh <version> <target-triple> <binary-path>
# Creates a distributable archive for parser-chunker.

VERSION="${1:?Usage: package.sh <version> <target-triple> <binary-path>}"
TARGET="${2:?Usage: package.sh <version> <target-triple> <binary-path>}"
BINARY="${3:?Usage: package.sh <version> <target-triple> <binary-path>}"

ARCHIVE_NAME="parser-chunker-${VERSION}-${TARGET}"
STAGING_DIR="dist/${ARCHIVE_NAME}"
DIST_DIR="dist"

echo "Packaging ${ARCHIVE_NAME}..."

# Clean and create staging directory
rm -rf "${STAGING_DIR}"
mkdir -p "${STAGING_DIR}/models"

# Copy binary
cp "${BINARY}" "${STAGING_DIR}/"

# Create models README
cat > "${STAGING_DIR}/models/README.md" << 'MODELS_EOF'
# Models Directory

Place ONNX model files here for vision/layout analysis features.

Required models (download separately):

- `layout-detection.onnx` — Document layout detection model
- `table-recognition.onnx` — Table structure recognition model

These models are not bundled with the release due to size constraints.
See the project README for download instructions.
MODELS_EOF

# Copy LICENSE if it exists
if [ -f "LICENSE" ]; then
    cp LICENSE "${STAGING_DIR}/"
else
    echo "Warning: LICENSE file not found" >&2
fi

# Create usage README
cat > "${STAGING_DIR}/README.md" << 'README_EOF'
# Parser Chunker

High-performance, air-gapped document parser and chunker.

## Quick Start

```bash
# Parse a single file
./parser-chunker document.pdf -o output/

# Parse with chunking
./parser-chunker document.pdf --chunk-strategy by-structure -o output/

# Parse a directory of files
./parser-chunker ./documents/ -o output/ --format jsonl

# Use accurate mode (layout detection)
./parser-chunker document.pdf --mode accurate --model-dir ./models/ -o output/
```

## Options

Run `./parser-chunker --help` for full usage information.

## Models

For accurate mode (vision/layout analysis), place ONNX model files in the
`models/` directory. See `models/README.md` for details.
README_EOF

# Generate SHA256 checksums
cd "${STAGING_DIR}"
if command -v sha256sum &>/dev/null; then
    find . -type f ! -name 'CHECKSUMS.sha256' -exec sha256sum {} \; > CHECKSUMS.sha256
elif command -v shasum &>/dev/null; then
    find . -type f ! -name 'CHECKSUMS.sha256' -exec shasum -a 256 {} \; > CHECKSUMS.sha256
else
    echo "Warning: no sha256sum or shasum available, skipping checksums" >&2
fi
cd - >/dev/null

# Create archive
mkdir -p "${DIST_DIR}"

case "${TARGET}" in
    *windows*)
        if command -v zip &>/dev/null; then
            cd dist
            zip -r "${ARCHIVE_NAME}.zip" "${ARCHIVE_NAME}/"
            cd - >/dev/null
        elif command -v 7z &>/dev/null; then
            cd dist
            7z a "${ARCHIVE_NAME}.zip" "${ARCHIVE_NAME}/"
            cd - >/dev/null
        else
            echo "Error: zip or 7z required for Windows archives" >&2
            exit 1
        fi
        ARCHIVE_FILE="${DIST_DIR}/${ARCHIVE_NAME}.zip"
        ;;
    *)
        tar -czf "${DIST_DIR}/${ARCHIVE_NAME}.tar.gz" -C dist "${ARCHIVE_NAME}"
        ARCHIVE_FILE="${DIST_DIR}/${ARCHIVE_NAME}.tar.gz"
        ;;
esac

# Generate checksum for the archive itself
cd "${DIST_DIR}"
ARCHIVE_BASENAME="$(basename "${ARCHIVE_FILE}")"
if command -v sha256sum &>/dev/null; then
    sha256sum "${ARCHIVE_BASENAME}" > "${ARCHIVE_BASENAME}.sha256"
elif command -v shasum &>/dev/null; then
    shasum -a 256 "${ARCHIVE_BASENAME}" > "${ARCHIVE_BASENAME}.sha256"
fi
cd - >/dev/null

# Cleanup staging
rm -rf "${STAGING_DIR}"

echo "Created: ${ARCHIVE_FILE}"
echo "Done."
