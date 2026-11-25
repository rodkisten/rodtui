//! End-to-end registry, schema, and declarative execution contracts.

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
fn registry_exposes_every_component_and_typed_argument() {
    let output = run(&["registry"], b"");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let registry: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("registry JSON");
    assert_eq!(registry["version"], 1);
    let components = registry["components"].as_array().expect("components");
    assert_eq!(components.len(), 3);
    assert!(components
        .iter()
        .any(|component| component["name"] == "choose"));
    assert!(components
        .iter()
        .any(|component| component["name"] == "confirm"));
    assert!(components
        .iter()
        .any(|component| component["name"] == "input"));
}

#[test]
fn component_schema_contains_enums_defaults_and_rejects_unknown_keys() {
    let output = run(&["schema", "--component", "confirm"], b"");
    assert!(output.status.success());
    let schema: serde_json::Value = serde_json::from_slice(&output.stdout).expect("schema JSON");
    assert_eq!(
        schema["oneOf"][0]["properties"]["component"]["const"],
        "confirm"
    );
    let args = &schema["oneOf"][0]["properties"]["args"];
    assert_eq!(args["additionalProperties"], false);
    assert_eq!(args["properties"]["theme"]["default"], "rod");
    assert!(args["properties"]["theme"]["enum"]
        .as_array()
        .expect("theme enum")
        .contains(&serde_json::json!("dracula")));
}

#[test]
fn declarative_choose_runs_through_the_normal_pipeline() {
    let document = serde_json::json!({
        "version": 1,
        "component": "choose",
        "args": {
            "items": ["Deploy", "Build", "Test"],
            "filter": true,
            "query": "bld",
            "select_1": true,
            "output": "json",
            "style.theme": "dracula"
        }
    });
    let output = run(&["run", "-"], document.to_string().as_bytes());
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let selected: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("selection JSON");
    assert_eq!(selected["selected"][0]["value"], "Build");
}

#[test]
fn declarative_input_and_confirm_preserve_exit_and_output_contracts() {
    let input = serde_json::json!({
        "version": 1,
        "component": "input",
        "args": {
            "value": "  Rod  ",
            "accept": true,
            "required": true,
            "output": "json"
        }
    });
    let accepted = run(&["run", "-"], input.to_string().as_bytes());
    assert!(accepted.status.success());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&accepted.stdout).expect("input JSON")["value"],
        "Rod"
    );

    let confirm = serde_json::json!({
        "version": 1,
        "component": "confirm",
        "args": { "message": "Deploy?", "no": true, "output": "json" }
    });
    let rejected = run(&["run", "-"], confirm.to_string().as_bytes());
    assert_eq!(rejected.status.code(), Some(1));
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&rejected.stdout).expect("confirm JSON")
            ["confirmed"],
        false
    );
}

#[test]
fn dry_run_resolves_aliases_and_invalid_documents_fail_with_code_two() {
    let document = serde_json::json!({
        "version": 1,
        "component": "input",
        "args": { "viewport.width": 42, "style.theme": "nord", "accept": true }
    });
    let dry = run(&["run", "--dry-run", "-"], document.to_string().as_bytes());
    assert!(dry.status.success());
    let argv: Vec<String> = serde_json::from_slice(&dry.stdout).expect("argv JSON");
    assert!(argv.windows(2).any(|pair| pair == ["--width", "42"]));
    assert!(argv.windows(2).any(|pair| pair == ["--theme", "nord"]));

    let invalid = run(
        &["run", "-"],
        br#"{"version":1,"component":"input","args":{"unknown":true}}"#,
    );
    assert_eq!(invalid.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("unknown input property"));
}
