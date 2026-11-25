//! Multi-step declarative flows over the existing interactive components.
//!
//! A flow never introduces a second widget implementation. Each step is
//! interpolated, translated through the registry, parsed by Clap, and executed
//! through the same choose/confirm/input functions as direct CLI usage.

use std::{
    collections::{HashMap, HashSet},
    io,
    path::PathBuf,
};

use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::{
    catalog::{self, DeclarativeDocument},
    choice::ChoiceState,
    choose::{self, ChooseResult},
    form::{self, FormResult},
    input,
    Commands,
};

const FLOW_VERSION: u32 = 1;
const DEFAULT_MAX_STEPS: usize = 100;

/// Execute or inspect a multi-component flow document.
#[derive(Debug, Clone, Args)]
pub struct FlowArgs {
    /// Flow JSON path, or - to read the document from stdin.
    #[arg(value_name = "FILE", default_value = "-")]
    pub file: PathBuf,

    /// Validate the graph and print its plan without opening a terminal.
    #[arg(long)]
    pub dry_run: bool,

    /// Emit compact result JSON.
    #[arg(long)]
    pub compact: bool,

    /// Override the document's transition safety limit.
    #[arg(long)]
    pub max_steps: Option<usize>,
}

/// Versioned directed flow graph.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlowDocument {
    pub version: u32,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub start: Option<String>,
    #[serde(default)]
    pub variables: Map<String, Value>,
    pub steps: Vec<FlowStep>,
    #[serde(default = "default_max_steps")]
    pub max_steps: usize,
}

/// One interactive node and its transition rules.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlowStep {
    pub id: String,
    pub component: String,
    #[serde(default)]
    pub args: Map<String, Value>,
    #[serde(default)]
    pub when: Option<Condition>,
    #[serde(default)]
    pub save_as: Option<String>,
    #[serde(default)]
    pub sensitive: bool,
    #[serde(default)]
    pub next: Option<String>,
    #[serde(default)]
    pub next_on_false: Option<String>,
}

/// A condition over a `variables.*` or `steps.*` JSON path.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Condition {
    pub path: String,
    #[serde(default)]
    pub equals: Option<Value>,
    #[serde(default)]
    pub not_equals: Option<Value>,
    #[serde(default)]
    pub exists: Option<bool>,
}

/// Machine-readable final state written once after the flow completes.
#[derive(Debug, Clone, Serialize)]
pub struct FlowReport {
    pub version: u32,
    pub name: Option<String>,
    pub status: FlowStatus,
    pub exit_code: i32,
    pub order: Vec<String>,
    pub variables: Map<String, Value>,
    pub steps: Map<String, Value>,
}

/// Terminal flow states have stable serialized names.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FlowStatus {
    Planned,
    Completed,
    Rejected,
    NoMatch,
    Cancelled,
}

/// Report plus process status used by the CLI dispatcher.
#[derive(Debug)]
pub struct FlowExecution {
    pub report: FlowReport,
    pub exit_code: i32,
}

#[derive(Debug, Clone)]
struct FlowContext {
    variables: Map<String, Value>,
    steps: Map<String, Value>,
}

#[derive(Debug)]
enum StepOutcome {
    Accepted(Value),
    Confirmed(bool, Value),
    NoMatch(Value),
    Cancelled,
}

/// Parse and execute a flow, or return a validated plan for `--dry-run`.
pub fn run(args: &FlowArgs) -> io::Result<FlowExecution> {
    let bytes = catalog::read_source(&args.file)?;
    let document: FlowDocument = serde_json::from_slice(&bytes)
        .map_err(|error| invalid(format!("invalid flow JSON: {error}")))?;
    let graph = validate(&document)?;
    let max_steps = args.max_steps.unwrap_or(document.max_steps);
    if max_steps == 0 {
        return Err(invalid("flow max_steps must be greater than zero"));
    }

    if args.dry_run {
        return Ok(FlowExecution {
            exit_code: 0,
            report: FlowReport {
                version: FLOW_VERSION,
                name: document.name.clone(),
                status: FlowStatus::Planned,
                exit_code: 0,
                order: plan_order(&document, &graph),
                variables: document.variables,
                steps: Map::new(),
            },
        });
    }

    execute(document, graph, max_steps)
}

/// Serialize the report according to the CLI density flag.
pub fn report_json(execution: &FlowExecution, compact: bool) -> io::Result<Vec<u8>> {
    let mut bytes = if compact {
        serde_json::to_vec(&execution.report)
    } else {
        serde_json::to_vec_pretty(&execution.report)
    }
    .map_err(io::Error::other)?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn execute(
    document: FlowDocument,
    graph: HashMap<String, usize>,
    max_steps: usize,
) -> io::Result<FlowExecution> {
    let mut context = FlowContext {
        variables: document.variables.clone(),
        steps: Map::new(),
    };
    let mut report_variables = document.variables.clone();
    let mut report_steps = Map::new();
    let mut order = Vec::new();
    let mut cursor = document
        .start
        .as_ref()
        .map(|id| graph[id])
        .unwrap_or(0);

    for _ in 0..max_steps {
        let step = &document.steps[cursor];
        order.push(step.id.clone());

        if step
            .when
            .as_ref()
            .is_some_and(|condition| !condition_matches(condition, &context))
        {
            let skipped = json!({ "status": "skipped" });
            context.steps.insert(step.id.clone(), skipped.clone());
            report_steps.insert(step.id.clone(), skipped);
            match next_cursor(step.next.as_deref(), cursor, &document, &graph)? {
                Some(next) => {
                    cursor = next;
                    continue;
                }
                None => {
                    return completed(
                        document,
                        order,
                        report_variables,
                        report_steps,
                    );
                }
            }
        }

        let resolved_args = interpolate_map(&step.args, &context)?;
        let invocation = DeclarativeDocument {
            version: 1,
            component: step.component.clone(),
            args: resolved_args,
        };
        let (command, _) = catalog::resolve_document(&invocation)?;
        let outcome = execute_step(command)?;

        let (actual, visible, false_confirmation, terminal) = match outcome {
            StepOutcome::Accepted(value) => {
                let visible = visible_value(&value, step.sensitive);
                (value, visible, false, None)
            }
            StepOutcome::Confirmed(confirmed, value) => {
                let visible = visible_value(&value, step.sensitive);
                (value, visible, !confirmed, None)
            }
            StepOutcome::NoMatch(value) => {
                let visible = visible_value(&value, step.sensitive);
                (value, visible, false, Some((FlowStatus::NoMatch, 1)))
            }
            StepOutcome::Cancelled => {
                return terminal_report(
                    document,
                    FlowStatus::Cancelled,
                    130,
                    order,
                    report_variables,
                    report_steps,
                );
            }
        };

        context.steps.insert(step.id.clone(), actual.clone());
        report_steps.insert(step.id.clone(), visible.clone());
        if let Some(alias) = &step.save_as {
            context.variables.insert(alias.clone(), actual);
            report_variables.insert(alias.clone(), visible);
        }

        if let Some((status, code)) = terminal {
            return terminal_report(
                document,
                status,
                code,
                order,
                report_variables,
                report_steps,
            );
        }

        let target = if false_confirmation {
            step.next_on_false.as_deref()
        } else {
            step.next.as_deref()
        };
        if false_confirmation && target.is_none() {
            return terminal_report(
                document,
                FlowStatus::Rejected,
                1,
                order,
                report_variables,
                report_steps,
            );
        }
        match next_cursor(target, cursor, &document, &graph)? {
            Some(next) => cursor = next,
            None => {
                return completed(document, order, report_variables, report_steps);
            }
        }
    }

    Err(invalid(format!(
        "flow exceeded max_steps ({max_steps}); check transition cycles"
    )))
}

fn execute_step(command: Commands) -> io::Result<StepOutcome> {
    match command {
        Commands::Choose(args) => {
            let items = input::collect_items(&args)?;
            if items.is_empty() {
                return Ok(StepOutcome::NoMatch(json!({ "status": "no_match" })));
            }
            let state = ChoiceState::new(items, &args)
                .map_err(|message| io::Error::new(io::ErrorKind::InvalidInput, message))?;
            match choose::choose(&args, state)? {
                ChooseResult::Accepted(outcome) => {
                    let selected: Vec<Value> = outcome
                        .selected
                        .iter()
                        .map(|choice| {
                            json!({
                                "index": choice.original_index,
                                "value": choice.value
                            })
                        })
                        .collect();
                    let values: Vec<Value> = outcome
                        .selected
                        .iter()
                        .map(|choice| Value::String(choice.value.clone()))
                        .collect();
                    Ok(StepOutcome::Accepted(json!({
                        "status": "accepted",
                        "query": outcome.query,
                        "value": values.first().cloned().unwrap_or(Value::Null),
                        "values": values,
                        "selected": selected
                    })))
                }
                ChooseResult::NoMatch => {
                    Ok(StepOutcome::NoMatch(json!({ "status": "no_match" })))
                }
                ChooseResult::Cancelled => Ok(StepOutcome::Cancelled),
            }
        }
        Commands::Confirm(args) => match form::confirm(&args)? {
            FormResult::Accepted(confirmed) => Ok(StepOutcome::Confirmed(
                confirmed,
                json!({ "status": "accepted", "confirmed": confirmed }),
            )),
            FormResult::Cancelled => Ok(StepOutcome::Cancelled),
        },
        Commands::Input(args) => {
            let initial = form::initial_input(&args)?;
            match form::input(&args, initial)? {
                FormResult::Accepted(value) => Ok(StepOutcome::Accepted(json!({
                    "status": "accepted",
                    "value": value
                }))),
                FormResult::Cancelled => Ok(StepOutcome::Cancelled),
            }
        }
        _ => Err(invalid("flow steps only support choose, confirm, and input")),
    }
}

fn validate(document: &FlowDocument) -> io::Result<HashMap<String, usize>> {
    if document.version != FLOW_VERSION {
        return Err(invalid(format!(
            "unsupported flow version {}; expected {FLOW_VERSION}",
            document.version
        )));
    }
    if document.steps.is_empty() {
        return Err(invalid("flow requires at least one step"));
    }
    if document.max_steps == 0 {
        return Err(invalid("flow max_steps must be greater than zero"));
    }

    let mut graph = HashMap::new();
    for (index, step) in document.steps.iter().enumerate() {
        if step.id.is_empty()
            || !step
                .id
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
        {
            return Err(invalid(format!(
                "flow step id {:?} must use ASCII letters, numbers, _ or -",
                step.id
            )));
        }
        if graph.insert(step.id.clone(), index).is_some() {
            return Err(invalid(format!("duplicate flow step id {:?}", step.id)));
        }
        if !matches!(step.component.as_str(), "choose" | "confirm" | "input") {
            return Err(invalid(format!(
                "flow step {:?} uses unknown component {:?}",
                step.id, step.component
            )));
        }
    }

    if let Some(start) = &document.start {
        require_target("start", start, &graph)?;
    }
    for step in &document.steps {
        if let Some(target) = &step.next {
            require_target(&format!("step {:?} next", step.id), target, &graph)?;
        }
        if let Some(target) = &step.next_on_false {
            require_target(
                &format!("step {:?} next_on_false", step.id),
                target,
                &graph,
            )?;
        }
        if step.next_on_false.is_some() && step.component != "confirm" {
            return Err(invalid(format!(
                "step {:?} uses next_on_false but is not a confirm component",
                step.id
            )));
        }
        if let Some(alias) = &step.save_as {
            if alias.is_empty() {
                return Err(invalid(format!(
                    "step {:?} save_as must not be empty",
                    step.id
                )));
            }
        }
    }
    Ok(graph)
}

fn require_target(label: &str, target: &str, graph: &HashMap<String, usize>) -> io::Result<()> {
    if graph.contains_key(target) {
        Ok(())
    } else {
        Err(invalid(format!("{label} targets unknown step {target:?}")))
    }
}

fn next_cursor(
    explicit: Option<&str>,
    current: usize,
    document: &FlowDocument,
    graph: &HashMap<String, usize>,
) -> io::Result<Option<usize>> {
    if let Some(target) = explicit {
        return graph
            .get(target)
            .copied()
            .map(Some)
            .ok_or_else(|| invalid(format!("transition targets unknown step {target:?}")));
    }
    Ok((current + 1 < document.steps.len()).then_some(current + 1))
}

fn condition_matches(condition: &Condition, context: &FlowContext) -> bool {
    let value = lookup(context, &condition.path);
    if let Some(expected) = condition.exists {
        if value.is_some() != expected {
            return false;
        }
    }
    let Some(value) = value else {
        return condition.exists == Some(false)
            && condition.equals.is_none()
            && condition.not_equals.is_none();
    };
    if condition.equals.as_ref().is_some_and(|expected| value != expected) {
        return false;
    }
    if condition
        .not_equals
        .as_ref()
        .is_some_and(|expected| value == expected)
    {
        return false;
    }
    if condition.equals.is_none() && condition.not_equals.is_none() && condition.exists.is_none() {
        return truthy(value);
    }
    true
}

fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_f64().is_some_and(|value| value != 0.0),
        Value::String(value) => !value.is_empty(),
        Value::Array(value) => !value.is_empty(),
        Value::Object(value) => !value.is_empty(),
    }
}

fn interpolate_map(args: &Map<String, Value>, context: &FlowContext) -> io::Result<Map<String, Value>> {
    args.iter()
        .map(|(key, value)| Ok((key.clone(), interpolate(value, context)?)))
        .collect()
}

fn interpolate(value: &Value, context: &FlowContext) -> io::Result<Value> {
    match value {
        Value::String(template) => interpolate_string(template, context),
        Value::Array(values) => values
            .iter()
            .map(|value| interpolate(value, context))
            .collect::<io::Result<Vec<_>>>()
            .map(Value::Array),
        Value::Object(values) => values
            .iter()
            .map(|(key, value)| Ok((key.clone(), interpolate(value, context)?)))
            .collect::<io::Result<Map<_, _>>>()
            .map(Value::Object),
        value => Ok(value.clone()),
    }
}

fn interpolate_string(template: &str, context: &FlowContext) -> io::Result<Value> {
    if let Some(path) = exact_placeholder(template) {
        return lookup(context, path)
            .cloned()
            .ok_or_else(|| invalid(format!("unknown flow interpolation ${{{path}}}")));
    }

    let mut rendered = String::new();
    let mut remainder = template;
    while let Some(start) = remainder.find("${") {
        rendered.push_str(&remainder[..start]);
        let placeholder = &remainder[start + 2..];
        let end = placeholder.find('}').ok_or_else(|| {
            invalid(format!("unterminated flow interpolation in {template:?}"))
        })?;
        let path = &placeholder[..end];
        let value = lookup(context, path)
            .ok_or_else(|| invalid(format!("unknown flow interpolation ${{{path}}}")))?;
        rendered.push_str(&scalar_text(value)?);
        remainder = &placeholder[end + 1..];
    }
    rendered.push_str(remainder);
    Ok(Value::String(rendered))
}

fn exact_placeholder(template: &str) -> Option<&str> {
    template
        .strip_prefix("${")
        .and_then(|value| value.strip_suffix('}'))
        .filter(|value| !value.contains("${") && !value.contains('}'))
}

fn scalar_text(value: &Value) -> io::Result<String> {
    match value {
        Value::Null => Ok(String::new()),
        Value::Bool(value) => Ok(value.to_string()),
        Value::Number(value) => Ok(value.to_string()),
        Value::String(value) => Ok(value.clone()),
        _ => Err(invalid(
            "embedded flow interpolations require a string, number, boolean, or null",
        )),
    }
}

fn lookup<'a>(context: &'a FlowContext, path: &str) -> Option<&'a Value> {
    let mut parts = path.split('.');
    let root = parts.next()?;
    let mut value = match root {
        "variables" => parts.next().and_then(|key| context.variables.get(key))?,
        "steps" => parts.next().and_then(|key| context.steps.get(key))?,
        key => context.variables.get(key).or_else(|| context.steps.get(key))?,
    };
    for part in parts {
        value = match value {
            Value::Object(map) => map.get(part)?,
            Value::Array(values) => values.get(part.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(value)
}

fn visible_value(value: &Value, sensitive: bool) -> Value {
    if sensitive {
        json!({ "status": "accepted", "redacted": true })
    } else {
        value.clone()
    }
}

fn completed(
    document: FlowDocument,
    order: Vec<String>,
    variables: Map<String, Value>,
    steps: Map<String, Value>,
) -> io::Result<FlowExecution> {
    terminal_report(
        document,
        FlowStatus::Completed,
        0,
        order,
        variables,
        steps,
    )
}

fn terminal_report(
    document: FlowDocument,
    status: FlowStatus,
    exit_code: i32,
    order: Vec<String>,
    variables: Map<String, Value>,
    steps: Map<String, Value>,
) -> io::Result<FlowExecution> {
    Ok(FlowExecution {
        exit_code,
        report: FlowReport {
            version: FLOW_VERSION,
            name: document.name,
            status,
            exit_code,
            order,
            variables,
            steps,
        },
    })
}

fn plan_order(document: &FlowDocument, graph: &HashMap<String, usize>) -> Vec<String> {
    let mut order = Vec::new();
    let mut seen = HashSet::new();
    let mut cursor = document
        .start
        .as_ref()
        .map(|id| graph[id])
        .unwrap_or(0);
    while seen.insert(cursor) {
        let step = &document.steps[cursor];
        order.push(step.id.clone());
        let Some(next) = next_cursor(step.next.as_deref(), cursor, document, graph)
            .ok()
            .flatten()
        else {
            break;
        };
        cursor = next;
    }
    order
}

fn default_max_steps() -> usize {
    DEFAULT_MAX_STEPS
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Map};

    use super::{
        condition_matches, interpolate, validate, Condition, FlowContext, FlowDocument, FlowStep,
    };

    fn context() -> FlowContext {
        FlowContext {
            variables: serde_json::from_value(json!({ "environment": "production" }))
                .expect("variables"),
            steps: serde_json::from_value(json!({
                "release": { "value": "v1.2.3" },
                "approval": { "confirmed": true }
            }))
            .expect("steps"),
        }
    }

    #[test]
    fn interpolation_preserves_exact_types_and_renders_embedded_scalars() {
        let context = context();
        assert_eq!(
            interpolate(&json!("${steps.approval.confirmed}"), &context).expect("boolean"),
            json!(true)
        );
        assert_eq!(
            interpolate(
                &json!("Deploy ${steps.release.value} to ${variables.environment}?"),
                &context
            )
            .expect("text"),
            json!("Deploy v1.2.3 to production?")
        );
    }

    #[test]
    fn conditions_support_equality_existence_and_truthiness() {
        let context = context();
        assert!(condition_matches(
            &Condition {
                path: "variables.environment".into(),
                equals: Some(json!("production")),
                not_equals: None,
                exists: None,
            },
            &context
        ));
        assert!(condition_matches(
            &Condition {
                path: "steps.approval.confirmed".into(),
                equals: None,
                not_equals: None,
                exists: None,
            },
            &context
        ));
    }

    #[test]
    fn validation_rejects_duplicate_ids_and_missing_transitions() {
        let step = |id: &str| FlowStep {
            id: id.into(),
            component: "input".into(),
            args: Map::new(),
            when: None,
            save_as: None,
            sensitive: false,
            next: None,
            next_on_false: None,
        };
        let duplicate = FlowDocument {
            version: 1,
            name: None,
            start: None,
            variables: Map::new(),
            steps: vec![step("same"), step("same")],
            max_steps: 100,
        };
        assert!(validate(&duplicate)
            .expect_err("duplicate")
            .to_string()
            .contains("duplicate"));

        let mut missing = step("first");
        missing.next = Some("unknown".into());
        let document = FlowDocument {
            version: 1,
            name: None,
            start: None,
            variables: Map::new(),
            steps: vec![missing],
            max_steps: 100,
        };
        assert!(validate(&document)
            .expect_err("target")
            .to_string()
            .contains("unknown step"));
    }
}
