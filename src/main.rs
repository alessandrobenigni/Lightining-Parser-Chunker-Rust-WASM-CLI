use std::time::Instant;

use clap::Parser;
use tracing::{error, info, warn};

use parser_chunker::cli::{
    expand_argfile, Cli, Command, OutputCompat, EXIT_CONFIG_ERROR, EXIT_FAILURE, EXIT_PARTIAL,
    EXIT_SUCCESS,
};
use parser_chunker::config;
use parser_chunker::orchestrator;
use parser_chunker::output;

fn main() {
    let raw_args: Vec<String> = std::env::args().collect();
    let expanded_args = match expand_argfile(raw_args) {
        Ok(args) => args,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(EXIT_CONFIG_ERROR);
        }
    };

    let mut cli = Cli::parse_from(expanded_args);

    // Handle subcommands first
    if let Some(Command::Completions { shell }) = &cli.command {
        Cli::print_completions(*shell, &mut std::io::stdout());
        std::process::exit(EXIT_SUCCESS);
    }

    // Load and merge config file if --config is provided
    if let Some(ref config_path) = cli.config {
        match config::load_config(config_path) {
            Ok(cfg) => {
                config::merge_config(&mut cli, &cfg);
            }
            Err(e) => {
                eprintln!("error: {e}");
                std::process::exit(EXIT_CONFIG_ERROR);
            }
        }
    }

    // Initialize tracing
    if let Some(ref log_file_path) = cli.log_file {
        use tracing_subscriber::layer::SubscriberExt;
        use tracing_subscriber::util::SubscriberInitExt;

        let file = match std::fs::File::create(log_file_path) {
            Ok(f) => f,
            Err(e) => {
                eprintln!(
                    "error: Failed to create log file '{}': {}",
                    log_file_path.display(),
                    e
                );
                std::process::exit(EXIT_CONFIG_ERROR);
            }
        };

        let filter = tracing_subscriber::EnvFilter::new(cli.log_level.as_tracing_filter());
        let stderr_layer = tracing_subscriber::fmt::layer().with_writer(std::io::stderr);
        let file_layer = tracing_subscriber::fmt::layer()
            .with_writer(std::sync::Mutex::new(file))
            .with_ansi(false);

        tracing_subscriber::registry()
            .with(filter)
            .with(stderr_layer)
            .with(file_layer)
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(cli.log_level.as_tracing_filter())
            .init();
    }

    // Validate
    if let Err(errors) = cli.validate() {
        for err in &errors {
            eprintln!("error: {err}");
        }
        eprintln!(
            "\nFor more information, try '{} --help'.",
            env!("CARGO_PKG_NAME")
        );
        std::process::exit(EXIT_CONFIG_ERROR);
    }

    let worker_count = cli.workers.unwrap_or_else(rayon::current_num_threads);
    let input = cli.input.as_ref().expect("input required");
    let output_dir = cli.output.as_ref().expect("output required");

    // Debug output directory
    let debug_dir = if cli.debug {
        let dir = &cli.debug_output;
        if let Err(e) = std::fs::create_dir_all(dir) {
            eprintln!("error: Failed to create debug output dir '{}': {}", dir.display(), e);
            std::process::exit(EXIT_CONFIG_ERROR);
        }
        eprintln!("Debug output: {}", dir.display());
        Some(dir.clone())
    } else {
        None
    };

    // Startup banner
    eprintln!(
        "parser-chunker v{} | {} {} | {} workers | mode: {:?} | strategy: {:?}{}",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
        worker_count,
        cli.mode,
        cli.chunk_strategy,
        if cli.gpu { " | GPU" } else { "" },
    );
    eprintln!("Input:  {}", input.display());
    eprintln!("Output: {}", output_dir.display());
    eprintln!();

    // Collect input files
    let start = Instant::now();
    let files = match orchestrator::collect_input_files(input) {
        Ok(f) if f.is_empty() => {
            eprintln!("error: No parseable files found at '{}'", input.display());
            std::process::exit(EXIT_FAILURE);
        }
        Ok(f) => f,
        Err(e) => {
            eprintln!("error [{}]: {e}", e.code());
            std::process::exit(EXIT_FAILURE);
        }
    };

    eprintln!("Found {} files to process.", files.len());

    // Process batch
    let (successes, failures) = orchestrator::process_batch_with_debug(
        &files,
        &cli.chunk_strategy,
        cli.max_tokens,
        cli.overlap,
        worker_count,
        debug_dir.as_deref(),
    );

    let elapsed = start.elapsed();

    // Create output directory once (instead of per-file in write_output)
    if let Err(e) = std::fs::create_dir_all(output_dir) {
        eprintln!("error: Failed to create output dir '{}': {}", output_dir.display(), e);
        std::process::exit(EXIT_FAILURE);
    }

    // Write output for successful files
    let mut write_errors = 0;
    for (path, chunks) in &successes {
        let filename = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown");

        // If --output-compat is set, write compat output instead of normal format
        let write_result = if let Some(OutputCompat::Unstructured) = &cli.output_compat {
            output::compat::write_unstructured(chunks, output_dir, filename)
        } else {
            output::write_output(chunks, output_dir, filename, &cli.format)
        };

        if let Err(e) = write_result {
            error!("[{}] Failed to write output for '{}': {}", e.code(), path.display(), e);
            write_errors += 1;
        }
    }

    // Write debug summary if enabled
    if let Some(ref dbg_dir) = debug_dir {
        let summary = serde_json::json!({
            "total_files": files.len(),
            "succeeded": successes.len(),
            "failed": failures.len(),
            "write_errors": write_errors,
            "total_chunks": successes.iter().map(|(_, c)| c.len()).sum::<usize>(),
            "elapsed_secs": elapsed.as_secs_f64(),
            "strategy": format!("{:?}", cli.chunk_strategy),
            "max_tokens": cli.max_tokens,
            "overlap": cli.overlap,
        });
        let summary_path = dbg_dir.join("_summary.json");
        if let Err(e) = std::fs::write(
            &summary_path,
            serde_json::to_string_pretty(&summary).unwrap_or_default(),
        ) {
            warn!("Failed to write debug summary: {}", e);
        }
    }

    // Summary
    let total = files.len();
    let succeeded = successes.len() - write_errors;
    let failed = failures.len() + write_errors;
    let total_chunks: usize = successes.iter().map(|(_, chunks)| chunks.len()).sum();

    eprintln!();
    eprintln!("--- Results ---");
    eprintln!(
        "Processed: {}/{} files ({} chunks) in {:.2}s",
        succeeded,
        total,
        total_chunks,
        elapsed.as_secs_f64()
    );

    if succeeded > 0 {
        let docs_per_sec = succeeded as f64 / elapsed.as_secs_f64();
        eprintln!("Throughput: {:.1} files/sec", docs_per_sec);
    }

    if !failures.is_empty() {
        eprintln!();
        eprintln!("--- Failures ({}) ---", failures.len());
        for (path, err) in &failures {
            eprintln!("  FAIL [{}]: {} — {}", err.code(), path.display(), err);
        }
    }

    // Exit code
    if failed == 0 {
        info!(
            files = succeeded,
            chunks = total_chunks,
            elapsed_ms = elapsed.as_millis() as u64,
            "processing complete"
        );
        std::process::exit(EXIT_SUCCESS);
    } else if succeeded > 0 && !cli.strict {
        warn!(
            succeeded = succeeded,
            failed = failed,
            "partial failure"
        );
        std::process::exit(EXIT_PARTIAL);
    } else {
        error!(failed = failed, "all files failed");
        std::process::exit(EXIT_FAILURE);
    }
}
