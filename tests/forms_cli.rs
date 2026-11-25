//! Noninteractive shell contracts for confirm and input.

use std::{
    io::Write,
    process::{Command, Output, Stdio},
};

fn run(arguments: &[&str], stdin: &[u8]) -> Output {
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
        .expect("stdin")
        .write_all(stdin)
        .expect("write stdin");
    child.wait_with_output().expect("wait for rodtui")
}

#[test]
fn confirm_uses_boolean_exit_codes_and_optional_json() {
    let yes = run(&["confirm", "Deploy?", "--yes", "--output", "json"], b"");
    assert!(yes.status.success());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&yes.stdout).expect("yes JSON")["confirmed"],
        true
    );

    let no = run(&["confirm", "Deploy?", "--no", "--output", "text"], b"");
    assert_eq!(no.status.code(), Some(1));
    assert_eq!(no.stdout, b"false\n");
}

#[test]
fn input_validates_and_serializes_explicit_values_without_a_terminal() {
    let accepted = run(
        &[
            "input",
            "--value",
            "  Rod  ",
            "--accept",
            "--required",
            "--output",
            "json",
        ],
        b"",
    );
    assert!(accepted.status.success());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&accepted.stdout).expect("input JSON")["value"],
        "Rod"
    );

    let invalid = run(
        &["input", "--value", "ab", "--accept", "--min-length", "3"],
        b"",
    );
    assert_eq!(invalid.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("at least 3"));
}

#[test]
fn input_can_validate_a_pipeline_seed() {
    let output = run(
        &[
            "input",
            "--read-stdin",
            "--accept",
            "--required",
            "--output",
            "text",
        ],
        b"pipeline value\n",
    );
    assert!(output.status.success());
    assert_eq!(output.stdout, b"pipeline value\n");
}

#[test]
fn empty_choose_pipeline_is_a_quiet_no_match() {
    let output = run(&["choose", "--filter", "--multi"], b"");
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}
