use std::path::Path;

use serde::Deserialize;

use crate::cli::{ChunkStrategy, LogLevel, OutputFormat, ProcessingMode};

/// Global configuration file (TOML), mirrors CLI options.
/// All fields are optional; present values override CLI defaults.
#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub format: Option<OutputFormatConfig>,
    pub chunk_strategy: Option<ChunkStrategyConfig>,
    pub max_tokens: Option<usize>,
    pub overlap: Option<usize>,
    pub tokenizer: Option<String>,
    pub workers: Option<usize>,
    pub mode: Option<ProcessingModeConfig>,
    pub gpu: Option<bool>,
    pub strict: Option<bool>,
    pub log_level: Option<LogLevelConfig>,
    pub debug: Option<bool>,
    pub debug_output: Option<String>,
}

/// Per-document config sidecar (e.g. `report.pdf.parser-chunker.toml`).
#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct PerDocConfig {
    pub chunk_strategy: Option<ChunkStrategyConfig>,
    pub max_tokens: Option<usize>,
    pub overlap: Option<usize>,
    pub format: Option<OutputFormatConfig>,
}

// Serde-friendly enums that map to CLI enums
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "kebab-case")]
pub enum OutputFormatConfig {
    Json,
    Jsonl,
    Parquet,
    Markdown,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "kebab-case")]
pub enum ChunkStrategyConfig {
    ByStructure,
    ByTitle,
    ByPage,
    Fixed,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "kebab-case")]
pub enum ProcessingModeConfig {
    Fast,
    Accurate,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "kebab-case")]
pub enum LogLevelConfig {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl From<ChunkStrategyConfig> for ChunkStrategy {
    fn from(c: ChunkStrategyConfig) -> Self {
        match c {
            ChunkStrategyConfig::ByStructure => ChunkStrategy::ByStructure,
            ChunkStrategyConfig::ByTitle => ChunkStrategy::ByTitle,
            ChunkStrategyConfig::ByPage => ChunkStrategy::ByPage,
            ChunkStrategyConfig::Fixed => ChunkStrategy::Fixed,
        }
    }
}

impl From<OutputFormatConfig> for OutputFormat {
    fn from(c: OutputFormatConfig) -> Self {
        match c {
            OutputFormatConfig::Json => OutputFormat::Json,
            OutputFormatConfig::Jsonl => OutputFormat::Jsonl,
            OutputFormatConfig::Parquet => OutputFormat::Parquet,
            OutputFormatConfig::Markdown => OutputFormat::Markdown,
        }
    }
}

impl From<ProcessingModeConfig> for ProcessingMode {
    fn from(c: ProcessingModeConfig) -> Self {
        match c {
            ProcessingModeConfig::Fast => ProcessingMode::Fast,
            ProcessingModeConfig::Accurate => ProcessingMode::Accurate,
        }
    }
}

impl From<LogLevelConfig> for LogLevel {
    fn from(c: LogLevelConfig) -> Self {
        match c {
            LogLevelConfig::Error => LogLevel::Error,
            LogLevelConfig::Warn => LogLevel::Warn,
            LogLevelConfig::Info => LogLevel::Info,
            LogLevelConfig::Debug => LogLevel::Debug,
            LogLevelConfig::Trace => LogLevel::Trace,
        }
    }
}

/// Load a global config file from disk.
pub fn load_config(path: &Path) -> Result<Config, crate::Error> {
    let contents = std::fs::read_to_string(path)
        .map_err(|e| crate::Error::ConfigError(format!("Failed to read config '{}': {}", path.display(), e)))?;
    let config: Config = toml::from_str(&contents)
        .map_err(|e| crate::Error::ConfigError(format!("Failed to parse config '{}': {}", path.display(), e)))?;
    Ok(config)
}

/// Merge config values into CLI struct. CLI flags override config values:
/// only apply config when the CLI field is still at its default.
pub fn merge_config(cli: &mut crate::cli::Cli, config: &Config) {
    // max_tokens: default is 512
    if cli.max_tokens == 512 {
        if let Some(mt) = config.max_tokens {
            cli.max_tokens = mt;
        }
    }

    // overlap: default is 50
    if cli.overlap == 50 {
        if let Some(ov) = config.overlap {
            cli.overlap = ov;
        }
    }

    // tokenizer: default is "cl100k_base"
    if cli.tokenizer == "cl100k_base" {
        if let Some(ref tok) = config.tokenizer {
            cli.tokenizer = tok.clone();
        }
    }

    // workers: default is None
    if cli.workers.is_none() {
        cli.workers = config.workers;
    }

    // gpu: default is false
    if !cli.gpu {
        if let Some(gpu) = config.gpu {
            cli.gpu = gpu;
        }
    }

    // strict: default is false
    if !cli.strict {
        if let Some(strict) = config.strict {
            cli.strict = strict;
        }
    }

    // debug: default is false
    if !cli.debug {
        if let Some(debug) = config.debug {
            cli.debug = debug;
        }
    }

    // chunk_strategy: default is ByStructure (we check by matching)
    if matches!(cli.chunk_strategy, ChunkStrategy::ByStructure) {
        if let Some(ref cs) = config.chunk_strategy {
            cli.chunk_strategy = cs.clone().into();
        }
    }

    // format: default is Json
    if matches!(cli.format, OutputFormat::Json) {
        if let Some(ref f) = config.format {
            cli.format = f.clone().into();
        }
    }

    // mode: default is Accurate
    if matches!(cli.mode, ProcessingMode::Accurate) {
        if let Some(ref m) = config.mode {
            cli.mode = m.clone().into();
        }
    }

    // debug_output: default is "./parser-chunker-debug/"
    if cli.debug_output == std::path::Path::new("./parser-chunker-debug/") {
        if let Some(ref d) = config.debug_output {
            cli.debug_output = std::path::PathBuf::from(d);
        }
    }
}

/// Look for a per-document sidecar config file next to the document.
/// E.g., for `docs/report.pdf`, looks for `docs/report.pdf.parser-chunker.toml`.
pub fn load_per_doc_config(doc_path: &Path) -> Option<PerDocConfig> {
    let file_name = doc_path.file_name()?.to_str()?;
    let sidecar_name = format!("{}.parser-chunker.toml", file_name);
    let sidecar_path = doc_path.with_file_name(sidecar_name);

    if !sidecar_path.is_file() {
        return None;
    }

    let contents = std::fs::read_to_string(&sidecar_path).ok()?;
    let config: PerDocConfig = toml::from_str(&contents).ok()?;
    Some(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_load_config_basic() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("config.toml");
        let mut f = std::fs::File::create(&config_path).unwrap();
        writeln!(f, "max_tokens = 1024").unwrap();
        writeln!(f, "overlap = 100").unwrap();
        writeln!(f, r#"chunk_strategy = "by-title""#).unwrap();
        writeln!(f, "gpu = true").unwrap();
        drop(f);

        let config = load_config(&config_path).unwrap();
        assert_eq!(config.max_tokens, Some(1024));
        assert_eq!(config.overlap, Some(100));
        assert!(config.gpu.unwrap());
        assert!(matches!(config.chunk_strategy, Some(ChunkStrategyConfig::ByTitle)));
    }

    #[test]
    fn test_load_config_invalid_toml() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("bad.toml");
        std::fs::write(&config_path, "not valid [[[toml").unwrap();

        let result = load_config(&config_path);
        assert!(result.is_err());
    }

    #[test]
    fn test_load_config_nonexistent() {
        let result = load_config(Path::new("/nonexistent/config.toml"));
        assert!(result.is_err());
    }

    #[test]
    fn test_merge_config_applies_when_default() {
        use clap::Parser;

        let mut cli = crate::cli::Cli::parse_from(["parser-chunker", "--input", ".", "--output", "out"]);
        let config = Config {
            max_tokens: Some(1024),
            overlap: Some(100),
            chunk_strategy: Some(ChunkStrategyConfig::ByTitle),
            gpu: Some(true),
            strict: Some(true),
            ..Config::default()
        };

        merge_config(&mut cli, &config);

        assert_eq!(cli.max_tokens, 1024);
        assert_eq!(cli.overlap, 100);
        assert!(matches!(cli.chunk_strategy, ChunkStrategy::ByTitle));
        assert!(cli.gpu);
        assert!(cli.strict);
    }

    #[test]
    fn test_merge_config_cli_overrides_config() {
        use clap::Parser;

        // CLI explicitly sets max_tokens to 256
        let mut cli = crate::cli::Cli::parse_from([
            "parser-chunker", "--input", ".", "--output", "out", "--max-tokens", "256",
        ]);
        let config = Config {
            max_tokens: Some(1024),
            ..Config::default()
        };

        merge_config(&mut cli, &config);

        // CLI value should win
        assert_eq!(cli.max_tokens, 256);
    }

    #[test]
    fn test_per_doc_config_detection() {
        let dir = TempDir::new().unwrap();
        let doc = dir.path().join("report.pdf");
        std::fs::write(&doc, b"fake pdf").unwrap();

        // No sidecar yet
        assert!(load_per_doc_config(&doc).is_none());

        // Create sidecar
        let sidecar = dir.path().join("report.pdf.parser-chunker.toml");
        let mut f = std::fs::File::create(&sidecar).unwrap();
        writeln!(f, "max_tokens = 2048").unwrap();
        writeln!(f, r#"chunk_strategy = "fixed""#).unwrap();
        drop(f);

        let per_doc = load_per_doc_config(&doc).unwrap();
        assert_eq!(per_doc.max_tokens, Some(2048));
        assert!(matches!(per_doc.chunk_strategy, Some(ChunkStrategyConfig::Fixed)));
    }

    #[test]
    fn test_per_doc_config_empty_file() {
        let dir = TempDir::new().unwrap();
        let doc = dir.path().join("test.txt");
        std::fs::write(&doc, "hello").unwrap();

        let sidecar = dir.path().join("test.txt.parser-chunker.toml");
        std::fs::write(&sidecar, "").unwrap();

        let per_doc = load_per_doc_config(&doc).unwrap();
        assert!(per_doc.max_tokens.is_none());
        assert!(per_doc.chunk_strategy.is_none());
    }
}
