#!/usr/bin/env bash
set -euo pipefail

# Generate a CycloneDX 1.4 SBOM for parser-chunker.
# Prefers cargo-cyclonedx if installed, otherwise falls back to
# parsing Cargo.lock and generating a minimal SBOM.

OUTPUT_FILE="${1:-sbom.cdx.json}"

echo "Generating SBOM..." >&2

# Try cargo-cyclonedx first
if command -v cargo-cyclonedx &>/dev/null || cargo cyclonedx --help &>/dev/null 2>&1; then
    echo "Using cargo-cyclonedx..." >&2
    cargo cyclonedx --format json --output-file "${OUTPUT_FILE}"
    echo "SBOM written to ${OUTPUT_FILE}" >&2
    exit 0
fi

# Try the Rust-based generator
if cargo run --bin generate-sbom 2>/dev/null > "${OUTPUT_FILE}"; then
    echo "SBOM written to ${OUTPUT_FILE} (via generate-sbom binary)" >&2
    exit 0
fi

echo "Falling back to manual Cargo.lock parsing..." >&2

# Manual parsing of Cargo.lock
if [ ! -f "Cargo.lock" ]; then
    echo "Error: Cargo.lock not found. Run 'cargo generate-lockfile' first." >&2
    exit 1
fi

TIMESTAMP="$(date -u +%Y-%m-%dT%H:%M:%SZ 2>/dev/null || date -Iseconds)"

# Parse Cargo.lock and build components JSON array
COMPONENTS="[]"
CURRENT_NAME=""
CURRENT_VERSION=""

while IFS= read -r line; do
    if [[ "$line" == "name = "* ]]; then
        CURRENT_NAME="${line#name = \"}"
        CURRENT_NAME="${CURRENT_NAME%\"}"
    elif [[ "$line" == "version = "* ]]; then
        CURRENT_VERSION="${line#version = \"}"
        CURRENT_VERSION="${CURRENT_VERSION%\"}"
    elif [[ "$line" == "source = \"registry"* ]] && [ -n "$CURRENT_NAME" ] && [ -n "$CURRENT_VERSION" ]; then
        # This is a registry dependency (not a local crate)
        PURL="pkg:cargo/${CURRENT_NAME}@${CURRENT_VERSION}"
        COMPONENT="{\"type\":\"library\",\"name\":\"${CURRENT_NAME}\",\"version\":\"${CURRENT_VERSION}\",\"purl\":\"${PURL}\"}"
        if [ "$COMPONENTS" = "[]" ]; then
            COMPONENTS="[${COMPONENT}"
        else
            COMPONENTS="${COMPONENTS},${COMPONENT}"
        fi
        CURRENT_NAME=""
        CURRENT_VERSION=""
    fi
done < Cargo.lock

if [ "$COMPONENTS" != "[]" ]; then
    COMPONENTS="${COMPONENTS}]"
fi

# Add placeholder entries for future vendored C dependencies
MUPDF_PLACEHOLDER="{\"type\":\"library\",\"name\":\"mupdf\",\"version\":\"0.0.0-placeholder\",\"purl\":\"pkg:generic/mupdf@0.0.0-placeholder\",\"description\":\"Placeholder: MuPDF C library (not yet vendored)\"}"
ONNX_PLACEHOLDER="{\"type\":\"library\",\"name\":\"onnxruntime\",\"version\":\"0.0.0-placeholder\",\"purl\":\"pkg:generic/onnxruntime@0.0.0-placeholder\",\"description\":\"Placeholder: ONNX Runtime C library (not yet vendored)\"}"

# Insert placeholders
COMPONENTS="${COMPONENTS%]},${MUPDF_PLACEHOLDER},${ONNX_PLACEHOLDER}]"

cat > "${OUTPUT_FILE}" << SBOM_EOF
{
  "bomFormat": "CycloneDX",
  "specVersion": "1.4",
  "version": 1,
  "metadata": {
    "timestamp": "${TIMESTAMP}",
    "component": {
      "type": "application",
      "name": "parser-chunker",
      "version": "0.1.0",
      "purl": "pkg:cargo/parser-chunker@0.1.0",
      "licenses": [{"license": {"id": "AGPL-3.0-only"}}]
    },
    "tools": [{"name": "generate-sbom.sh", "version": "1.0.0"}]
  },
  "components": ${COMPONENTS}
}
SBOM_EOF

echo "SBOM written to ${OUTPUT_FILE}" >&2
