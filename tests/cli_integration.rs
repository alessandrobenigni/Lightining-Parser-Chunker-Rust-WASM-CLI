use assert_cmd::Command;
use predicates::prelude::*;

fn cmd() -> Command {
    Command::cargo_bin("parser-chunker").unwrap()
}

#[test]
fn test_help_shows_grouped_headings() {
    let output = cmd().arg("--help").output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Input/Output"), "Missing 'Input/Output' heading in help:\n{stdout}");
    assert!(stdout.contains("Chunking"), "Missing 'Chunking' heading in help:\n{stdout}");
    assert!(stdout.contains("Execution"), "Missing 'Execution' heading in help:\n{stdout}");
    assert!(stdout.contains("Logging & Debug"), "Missing 'Logging & Debug' heading in help:\n{stdout}");
}

#[test]
fn test_version_output() {
    let output = cmd().arg("--version").output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("parser-chunker"), "Version output missing binary name:\n{stdout}");
}

#[test]
fn test_invalid_overlap_exit_code() {
    let assert = cmd()
        .args(["--input", ".", "--output", "out", "--overlap", "600", "--max-tokens", "512"])
        .assert();
    // Should exit with EXIT_CONFIG_ERROR (3)
    assert.code(3);
}

#[test]
fn test_completions_bash() {
    let assert = cmd().args(["completions", "bash"]).assert();
    assert.success().stdout(predicate::str::contains("_parser-chunker"));
}

#[test]
fn test_missing_input_exit_code() {
    // No --input or --output provided, no subcommand
    let assert = cmd().assert();
    // Should exit with EXIT_CONFIG_ERROR (3) from our validate()
    assert.code(3);
}
