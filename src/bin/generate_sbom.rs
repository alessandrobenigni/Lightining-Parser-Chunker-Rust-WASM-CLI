//! Generates a CycloneDX 1.4 SBOM from Cargo.lock.
//!
//! Usage: cargo run --bin generate-sbom > sbom.cdx.json

use std::fs;
use std::io::{self, Write};

fn main() {
    let lock_contents = match fs::read_to_string("Cargo.lock") {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error reading Cargo.lock: {e}");
            eprintln!("Run this from the project root directory.");
            std::process::exit(1);
        }
    };

    let packages = parse_cargo_lock(&lock_contents);
    let sbom = build_cyclonedx(&packages);

    let stdout = io::stdout();
    let mut out = stdout.lock();
    if let Err(e) = out.write_all(sbom.as_bytes()) {
        eprintln!("Error writing SBOM: {e}");
        std::process::exit(1);
    }
}

struct Package {
    name: String,
    version: String,
    source: Option<String>,
}

fn parse_cargo_lock(contents: &str) -> Vec<Package> {
    let mut packages = Vec::new();
    let mut current_name: Option<String> = None;
    let mut current_version: Option<String> = None;
    let mut current_source: Option<String> = None;

    for line in contents.lines() {
        let line = line.trim();

        if line == "[[package]]" {
            // Flush previous package
            if let (Some(name), Some(version)) = (current_name.take(), current_version.take()) {
                packages.push(Package {
                    name,
                    version,
                    source: current_source.take(),
                });
            }
            current_name = None;
            current_version = None;
            current_source = None;
        } else if let Some(rest) = line.strip_prefix("name = \"") {
            current_name = Some(rest.trim_end_matches('"').to_string());
        } else if let Some(rest) = line.strip_prefix("version = \"") {
            current_version = Some(rest.trim_end_matches('"').to_string());
        } else if let Some(rest) = line.strip_prefix("source = \"") {
            current_source = Some(rest.trim_end_matches('"').to_string());
        }
    }

    // Flush last package
    if let (Some(name), Some(version)) = (current_name, current_version) {
        packages.push(Package {
            name,
            version,
            source: current_source,
        });
    }

    packages
}

fn build_cyclonedx(packages: &[Package]) -> String {
    let mut components = Vec::new();

    for pkg in packages {
        // Only include registry dependencies, not the root crate
        let is_registry = pkg
            .source
            .as_ref()
            .is_some_and(|s| s.starts_with("registry"));

        if !is_registry {
            continue;
        }

        let purl = format!("pkg:cargo/{}@{}", pkg.name, pkg.version);
        components.push(format!(
            r#"    {{
      "type": "library",
      "name": "{}",
      "version": "{}",
      "purl": "{}"
    }}"#,
            escape_json(&pkg.name),
            escape_json(&pkg.version),
            escape_json(&purl),
        ));
    }

    // Add placeholder entries for future vendored C dependencies
    components.push(
        r#"    {
      "type": "library",
      "name": "mupdf",
      "version": "0.0.0-placeholder",
      "purl": "pkg:generic/mupdf@0.0.0-placeholder",
      "description": "Placeholder: MuPDF C library (not yet vendored)"
    }"#
        .to_string(),
    );
    components.push(
        r#"    {
      "type": "library",
      "name": "onnxruntime",
      "version": "0.0.0-placeholder",
      "purl": "pkg:generic/onnxruntime@0.0.0-placeholder",
      "description": "Placeholder: ONNX Runtime C library (not yet vendored)"
    }"#
        .to_string(),
    );

    let components_json = components.join(",\n");
    let timestamp = chrono_free_timestamp();

    format!(
        r#"{{
  "bomFormat": "CycloneDX",
  "specVersion": "1.4",
  "version": 1,
  "metadata": {{
    "timestamp": "{timestamp}",
    "component": {{
      "type": "application",
      "name": "parser-chunker",
      "version": "0.1.0",
      "purl": "pkg:cargo/parser-chunker@0.1.0",
      "licenses": [{{"license": {{"id": "AGPL-3.0-only"}}}}]
    }},
    "tools": [{{"name": "generate-sbom", "version": "1.0.0"}}]
  }},
  "components": [
{components_json}
  ]
}}
"#
    )
}

/// Minimal JSON string escaping.
fn escape_json(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

/// Generate an ISO 8601 UTC timestamp without pulling in chrono.
fn chrono_free_timestamp() -> String {
    // Use a fixed format placeholder; in practice, the build timestamp
    // can be injected via env var or the shell wrapper.
    match std::env::var("SBOM_TIMESTAMP") {
        Ok(ts) => ts,
        Err(_) => "1970-01-01T00:00:00Z".to_string(),
    }
}
