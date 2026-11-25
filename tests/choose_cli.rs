//! End-to-end CLI contracts for noninteractive `choose` pipelines.
//!
//! `--select-1` deliberately bypasses terminal rendering after fuzzy matching,
//! which lets these tests exercise real stdin/stdout/exit-code behavior without
//! a pseudo-terminal.

use std::{
    fs,
    io::Write,
    process::{Command, Output, Stdio},
};

fn run_with_stdin(arguments: &[&str], input: &[u8]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_rodtui"))
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn rodtui");
    child
        .stdin
        .take()
        .expect("piped stdin")
        .write_all(input)
        .expect("write stdin");
    child.wait_with_output().expect("wait for rodtui")
}

#[test]
fn piped_lines_fuzzy_filter_to_json() {
    let output = run_with_stdin(
        &[
            "choose",
            "--filter",
            "--query",
            "bld",
            "--select-1",
            "--output",
            "json",
        ],
        b"Deploy\nBuild\nTest\n",
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("JSON stdout");
    assert_eq!(json["query"], "bld");
    assert_eq!(json["selected"][0]["index"], 1);
    assert_eq!(json["selected"][0]["value"], "Build");
}

#[test]
fn null_delimited_input_and_output_preserve_records() {
    let output = run_with_stdin(
        &[
            "choose",
            "--read0",
            "--dedupe",
            "--filter",
            "--query",
            "prod",
            "--select-1",
            "--print0",
        ],
        b"Production\0Staging\0Production\0",
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"Production\0");
}

#[test]
fn unique_positional_match_can_print_original_index() {
    let output = run_with_stdin(
        &[
            "choose",
            "Deploy",
            "Build",
            "Test",
            "--no-stdin",
            "--filter",
            "--query",
            "test",
            "--select-1",
            "--output",
            "index",
        ],
        b"",
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"2\n");
}

#[test]
fn repeatable_file_input_uses_the_same_fuzzy_pipeline() {
    let mut path = std::env::temp_dir();
    path.push(format!("rodtui-choose-{}.txt", std::process::id()));
    fs::write(&path, b"production\nstaging\npreview\n").expect("write fixture");
    let path_text = path.to_string_lossy().into_owned();
    let output = run_with_stdin(
        &[
            "choose",
            "--input-file",
            &path_text,
            "--no-stdin",
            "--filter",
            "--query",
            "stag",
            "--select-1",
        ],
        b"",
    );
    let _ = fs::remove_file(&path);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"staging\n");
}

#[test]
fn no_match_and_invalid_input_use_distinct_exit_codes() {
    let no_match = run_with_stdin(
        &["choose", "--filter", "--query", "missing", "--select-1"],
        b"Deploy\nBuild\nTest\n",
    );
    assert_eq!(no_match.status.code(), Some(1));
    assert!(no_match.stdout.is_empty());

    let invalid = run_with_stdin(&["choose", "--no-stdin", "--select-1"], b"");
    assert_eq!(invalid.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("requires at least one item"));
}
