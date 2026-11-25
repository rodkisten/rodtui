//! End-to-end noninteractive contracts for the flow engine.

use std::{
    io::Write,
    process::{Command, Output, Stdio},
};

fn run(arguments: &[&str], document: serde_json::Value) -> Output {
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
        .write_all(document.to_string().as_bytes())
        .expect("write flow");
    child.wait_with_output().expect("wait for rodtui")
}

fn report(output: &Output) -> serde_json::Value {
    serde_json::from_slice(&output.stdout).expect("flow report JSON")
}

#[test]
fn flow_shares_values_interpolates_and_preserves_component_results() {
    let document = serde_json::json!({
        "version": 1,
        "name": "release",
        "variables": { "environment": "production" },
        "steps": [
            {
                "id": "release",
                "component": "input",
                "save_as": "release",
                "args": { "value": "v1.2.3", "accept": true, "required": true }
            },
            {
                "id": "action",
                "component": "choose",
                "args": {
                    "items": ["Deploy", "Build", "Test"],
                    "filter": true,
                    "query": "dpl",
                    "select_1": true
                }
            },
            {
                "id": "approval",
                "component": "confirm",
                "args": {
                    "message": "Deploy ${variables.release.value} to ${variables.environment}?",
                    "yes": true
                }
            }
        ]
    });
    let output = run(&["flow", "-"], document);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report = report(&output);
    assert_eq!(report["status"], "completed");
    assert_eq!(report["order"], serde_json::json!(["release", "action", "approval"]));
    assert_eq!(report["variables"]["release"]["value"], "v1.2.3");
    assert_eq!(report["steps"]["action"]["value"], "Deploy");
    assert_eq!(report["steps"]["approval"]["confirmed"], true);
}

#[test]
fn negative_confirmation_can_branch_instead_of_rejecting_the_flow() {
    let document = serde_json::json!({
        "version": 1,
        "steps": [
            {
                "id": "approval",
                "component": "confirm",
                "next": "done",
                "next_on_false": "reason",
                "args": { "message": "Deploy?", "no": true }
            },
            {
                "id": "reason",
                "component": "input",
                "next": "done",
                "args": { "value": "waiting for QA", "accept": true }
            },
            {
                "id": "done",
                "component": "input",
                "args": { "value": "recorded", "accept": true }
            }
        ]
    });
    let output = run(&["flow", "-"], document);
    assert!(output.status.success());
    let report = report(&output);
    assert_eq!(report["order"], serde_json::json!(["approval", "reason", "done"]));
    assert_eq!(report["steps"]["approval"]["confirmed"], false);
}

#[test]
fn conditions_skip_steps_and_sensitive_values_are_redacted() {
    let document = serde_json::json!({
        "version": 1,
        "variables": { "environment": "staging" },
        "steps": [
            {
                "id": "token",
                "component": "input",
                "save_as": "token",
                "sensitive": true,
                "args": { "value": "super-secret", "accept": true, "required": true }
            },
            {
                "id": "production_only",
                "component": "confirm",
                "when": {
                    "path": "variables.environment",
                    "equals": "production"
                },
                "args": { "yes": true }
            },
            {
                "id": "uses_secret",
                "component": "confirm",
                "args": {
                    "message": "Token ${variables.token.value} loaded?",
                    "yes": true
                }
            }
        ]
    });
    let output = run(&["flow", "-"], document);
    assert!(output.status.success());
    let report = report(&output);
    assert_eq!(report["steps"]["production_only"]["status"], "skipped");
    assert_eq!(report["steps"]["token"]["redacted"], true);
    assert_eq!(report["variables"]["token"]["redacted"], true);
    assert!(!String::from_utf8_lossy(&output.stdout).contains("super-secret"));
}

#[test]
fn unhandled_negative_confirmation_rejects_with_report_and_exit_one() {
    let document = serde_json::json!({
        "version": 1,
        "steps": [{
            "id": "approval",
            "component": "confirm",
            "args": { "message": "Deploy?", "no": true }
        }]
    });
    let output = run(&["flow", "-"], document);
    assert_eq!(output.status.code(), Some(1));
    let report = report(&output);
    assert_eq!(report["status"], "rejected");
    assert_eq!(report["exit_code"], 1);
}

#[test]
fn dry_run_validates_graph_and_transition_limit_stops_cycles() {
    let plan = serde_json::json!({
        "version": 1,
        "start": "first",
        "steps": [
            { "id": "first", "component": "input", "next": "second" },
            { "id": "second", "component": "confirm", "next": "first" }
        ]
    });
    let dry = run(&["flow", "--dry-run", "-"], plan.clone());
    assert!(dry.status.success());
    assert_eq!(report(&dry)["status"], "planned");
    assert_eq!(report(&dry)["order"], serde_json::json!(["first", "second"]));

    let cycled = serde_json::json!({
        "version": 1,
        "max_steps": 2,
        "start": "first",
        "steps": [
            {
                "id": "first",
                "component": "input",
                "next": "second",
                "args": { "value": "one", "accept": true }
            },
            {
                "id": "second",
                "component": "input",
                "next": "first",
                "args": { "value": "two", "accept": true }
            }
        ]
    });
    let output = run(&["flow", "-"], cycled);
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("exceeded max_steps"));
}

#[test]
fn invalid_graphs_fail_before_any_component_opens() {
    let document = serde_json::json!({
        "version": 1,
        "steps": [{
            "id": "start",
            "component": "input",
            "next": "missing"
        }]
    });
    let output = run(&["flow", "-"], document);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unknown step"));
}
