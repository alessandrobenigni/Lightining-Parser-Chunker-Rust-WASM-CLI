# Security Policy

## Reporting Vulnerabilities

If you discover a security vulnerability in Lightning Parser Chunker, please report it responsibly:

1. **Do NOT open a public issue**
2. Email: security@alessandrobenigni.com
3. Include: description, reproduction steps, impact assessment

## Response Timeline

- **Acknowledgment:** Within 24 hours
- **Assessment:** Within 72 hours
- **Patch (critical):** Within 7 days
- **Patch (high):** Within 30 days

## Security Properties

Lightning Parser Chunker is designed for air-gapped, high-security environments:

- **Zero network calls** — verifiable via `strace -e trace=network`
- **No telemetry** — ONNX Runtime built with telemetry disabled
- **Signed binaries** — platform-native code signing on all releases
- **SBOM included** — CycloneDX Software Bill of Materials in every release
- **Reproducible builds** — Dockerfile provided for build verification

## Supported Versions

| Version | Supported |
|---------|-----------|
| 0.1.x   | Yes       |
