use std::path::PathBuf;

use clap::{CommandFactory, Parser, Subcommand, ValueEnum};

// Exit code constants
pub const EXIT_SUCCESS: i32 = 0;
pub const EXIT_PARTIAL: i32 = 1;
pub const EXIT_FAILURE: i32 = 2;
pub const EXIT_CONFIG_ERROR: i32 = 3;

#[derive(Debug, Clone, ValueEnum)]
pub enum OutputFormat {
    Json,
    Jsonl,
    Parquet,
    Markdown,
}

#[derive(Debug, Clone, ValueEnum)]
pub enum ChunkStrategy {
    ByStructure,
    ByTitle,
    ByPage,
    Fixed,
}

#[derive(Debug, Clone, ValueEnum, PartialEq, Eq)]
pub enum ProcessingMode {
    Fast,
    Accurate,
}

#[derive(Debug, Clone, ValueEnum)]
pub enum OutputCompat {
    Unstructured,
}

#[derive(Debug, Clone, ValueEnum)]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl LogLevel {
    pub fn as_tracing_filter(&self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warn => "warn",
            Self::Info => "info",
            Self::Debug => "debug",
            Self::Trace => "trace",
        }
    }
}

#[derive(Debug, Parser)]
#[command(
    name = "parser-chunker",
    version,
    about = "The fastest document parser and chunker for RAG pipelines",
    long_about = "Lightning Parser Chunker — the fastest document parser for RAG pipelines. \
                  314x faster than Unstructured. Single compiled Rust binary, zero external dependencies. \
                  14 formats, 4 chunking strategies, 5 output formats. Air-gapped by design."
)]
pub struct Cli {
    // === Input/Output ===
    /// Input file or directory to parse
    #[arg(short, long, help_heading = "Input/Output")]
    pub input: Option<PathBuf>,

    /// Output directory for results
    #[arg(short, long, help_heading = "Input/Output")]
    pub output: Option<PathBuf>,

    /// Output format
    #[arg(short, long, default_value = "json", help_heading = "Input/Output")]
    pub format: OutputFormat,

    /// Output compatibility mode (produce output matching another tool's schema)
    #[arg(long, help_heading = "Input/Output")]
    pub output_compat: Option<OutputCompat>,

    // === Chunking ===
    /// Chunking strategy
    #[arg(short, long, default_value = "by-structure", help_heading = "Chunking")]
    pub chunk_strategy: ChunkStrategy,

    /// Maximum tokens per chunk
    #[arg(long, default_value = "512", help_heading = "Chunking")]
    pub max_tokens: usize,

    /// Overlap tokens between chunks
    #[arg(long, default_value = "50", help_heading = "Chunking")]
    pub overlap: usize,

    /// Tokenizer model for token counting
    #[arg(long, default_value = "cl100k_base", help_heading = "Chunking")]
    pub tokenizer: String,

    // === Execution ===
    /// Number of worker threads (defaults to CPU count)
    #[arg(short, long, help_heading = "Execution")]
    pub workers: Option<usize>,

    /// Processing mode (fast uses lightweight models, accurate uses full models)
    #[arg(short, long, default_value = "accurate", help_heading = "Execution")]
    pub mode: ProcessingMode,

    /// Enable GPU acceleration for vision/OCR pipeline
    #[arg(long, help_heading = "Execution")]
    pub gpu: bool,

    /// Abort on first document failure instead of continuing
    #[arg(long, help_heading = "Execution")]
    pub strict: bool,

    /// Path to TOML configuration file
    #[arg(long, help_heading = "Execution")]
    pub config: Option<PathBuf>,

    // === Logging & Debug ===
    /// Log level
    #[arg(long, default_value = "warn", help_heading = "Logging & Debug")]
    pub log_level: LogLevel,

    /// Path to log file (logs are written to stderr by default)
    #[arg(long, help_heading = "Logging & Debug")]
    pub log_file: Option<PathBuf>,

    /// Enable debug output with intermediate pipeline inspection
    #[arg(long, help_heading = "Logging & Debug")]
    pub debug: bool,

    /// Debug output directory
    #[arg(long, default_value = "./parser-chunker-debug/", help_heading = "Logging & Debug")]
    pub debug_output: PathBuf,

    /// Subcommand (e.g., completions)
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Generate shell completions and print to stdout
    Completions {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
}

impl Cli {
    /// Generate shell completions for the given shell and write to the given writer.
    pub fn print_completions(shell: clap_complete::Shell, writer: &mut impl std::io::Write) {
        clap_complete::generate(shell, &mut Self::command(), "parser-chunker", writer);
    }
}

/// Expand `@argfile` arguments: replace `@path` with the contents of the file.
/// Lines starting with `#` are treated as comments. Blank lines are skipped.
pub fn expand_argfile(args: Vec<String>) -> Result<Vec<String>, String> {
    let mut expanded = Vec::with_capacity(args.len());

    for arg in args {
        if let Some(path_str) = arg.strip_prefix('@') {
            let path = std::path::Path::new(path_str);
            let contents = std::fs::read_to_string(path)
                .map_err(|e| format!("Failed to read argfile '{}': {}", path_str, e))?;

            for line in contents.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() || trimmed.starts_with('#') {
                    continue;
                }
                expanded.push(trimmed.to_string());
            }
        } else {
            expanded.push(arg);
        }
    }

    Ok(expanded)
}

impl Cli {
    /// Validate cross-flag combinations. Returns all violations (not just the first).
    /// Should only be called when no subcommand is active (subcommands handle their own args).
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        // Rule 0: input and output are required for the main command
        if self.input.is_none() {
            errors.push("--input is required. Specify a file or directory to parse.".to_string());
        }
        if self.output.is_none() {
            errors.push("--output is required. Specify an output directory.".to_string());
        }

        // Rule 1: overlap must be less than max_tokens
        if self.overlap >= self.max_tokens {
            errors.push(format!(
                "--overlap ({}) must be less than --max-tokens ({}). \
                 Overlap >= max_tokens produces degenerate chunks.",
                self.overlap, self.max_tokens
            ));
        }

        // Rule 2: --debug-output without --debug is a warning (collected as error for strictness)
        if self.debug_output != *"./parser-chunker-debug/" && !self.debug {
            errors.push(
                "--debug-output is set but --debug is not enabled. \
                 Add --debug to enable debug output, or remove --debug-output."
                    .to_string(),
            );
        }

        // Rule 3: workers must be >= 1 if explicitly set
        if let Some(w) = self.workers {
            if w == 0 {
                errors.push(
                    "--workers must be at least 1. Use --workers 1 for single-threaded processing."
                        .to_string(),
                );
            }
        }

        // Rule 4: --gpu + --mode fast is invalid (GPU only for vision/OCR in accurate mode)
        if self.gpu && self.mode == ProcessingMode::Fast {
            errors.push(
                "--gpu is only supported with --mode accurate. \
                 Fast mode uses lightweight CPU models that do not benefit from GPU acceleration."
                    .to_string(),
            );
        }

        // Rule 5: --config path must exist and be a file
        if let Some(ref config_path) = self.config {
            if !config_path.exists() {
                errors.push(format!(
                    "--config path does not exist: {}",
                    config_path.display()
                ));
            } else if !config_path.is_file() {
                errors.push(format!(
                    "--config path is not a file: {}",
                    config_path.display()
                ));
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn make_cli(args: &[&str]) -> Cli {
        let mut full_args = vec!["parser-chunker"];
        full_args.extend_from_slice(args);
        Cli::parse_from(full_args)
    }

    #[test]
    fn test_validate_all_defaults_ok() {
        let cli = make_cli(&["--input", ".", "--output", "out"]);
        assert!(cli.validate().is_ok());
    }

    #[test]
    fn test_validate_overlap_exceeds_max_tokens() {
        let cli = make_cli(&["--input", ".", "--output", "out", "--overlap", "600", "--max-tokens", "512"]);
        let errs = cli.validate().unwrap_err();
        assert!(errs.iter().any(|e| e.contains("--overlap")));
    }

    #[test]
    fn test_validate_workers_zero() {
        let cli = make_cli(&["--input", ".", "--output", "out", "--workers", "0"]);
        let errs = cli.validate().unwrap_err();
        assert!(errs.iter().any(|e| e.contains("--workers")));
    }

    #[test]
    fn test_validate_gpu_fast_mode() {
        let cli = make_cli(&["--input", ".", "--output", "out", "--gpu", "--mode", "fast"]);
        let errs = cli.validate().unwrap_err();
        assert!(errs.iter().any(|e| e.contains("--gpu")));
    }

    #[test]
    fn test_validate_config_nonexistent() {
        let cli = make_cli(&["--input", ".", "--output", "out", "--config", "/nonexistent/path.toml"]);
        let errs = cli.validate().unwrap_err();
        assert!(errs.iter().any(|e| e.contains("--config")));
    }

    #[test]
    fn test_validate_multiple_errors() {
        let cli = make_cli(&[
            "--input", ".", "--output", "out",
            "--overlap", "600", "--max-tokens", "512",
            "--workers", "0",
        ]);
        let errs = cli.validate().unwrap_err();
        assert!(errs.len() >= 2, "Expected multiple errors, got {}", errs.len());
    }

    #[test]
    fn test_validate_input_required() {
        let cli = make_cli(&["--output", "out"]);
        let errs = cli.validate().unwrap_err();
        assert!(errs.iter().any(|e| e.contains("--input")));
    }

    #[test]
    fn test_argfile_expansion() {
        let dir = tempfile::tempdir().unwrap();
        let argfile = dir.path().join("args.txt");
        let mut f = std::fs::File::create(&argfile).unwrap();
        writeln!(f, "# comment").unwrap();
        writeln!(f, "--input").unwrap();
        writeln!(f, "./docs").unwrap();
        writeln!(f).unwrap();
        writeln!(f, "--output").unwrap();
        writeln!(f, "./out").unwrap();
        drop(f);

        let args = vec![
            "parser-chunker".to_string(),
            format!("@{}", argfile.display()),
        ];
        let expanded = expand_argfile(args).unwrap();
        assert_eq!(expanded, vec!["parser-chunker", "--input", "./docs", "--output", "./out"]);
    }

    #[test]
    fn test_argfile_missing_file() {
        let args = vec!["parser-chunker".to_string(), "@/nonexistent/file.txt".to_string()];
        assert!(expand_argfile(args).is_err());
    }

    #[test]
    fn test_argfile_no_at_args() {
        let args = vec!["parser-chunker".to_string(), "--input".to_string(), ".".to_string()];
        let expanded = expand_argfile(args.clone()).unwrap();
        assert_eq!(expanded, args);
    }

    #[test]
    fn test_completions_subcommand_parses() {
        let cli = make_cli(&["completions", "bash"]);
        assert!(matches!(cli.command, Some(Command::Completions { .. })));
    }
}
