//! Runtime registry, JSON Schema generation, and declarative execution.
//!
//! Clap remains the executable source of truth. This module inspects the same
//! command graph used by the CLI, serializes component metadata, derives a
//! JSON Schema, and converts validated JSON properties back into CLI arguments.

use std::{
    collections::BTreeMap,
    fs,
    io::{self, IsTerminal, Read},
    path::PathBuf,
};

use clap::{Arg, ArgAction, Args, Command, CommandFactory, Parser};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::{Cli, Commands};

const REGISTRY_VERSION: u32 = 1;
const COMPONENTS: [&str; 3] = ["choose", "confirm", "input"];

/// Print machine-readable metadata for all interactive components.
#[derive(Debug, Clone, Args)]
pub struct RegistryArgs {
    /// Limit output to one component.
    #[arg(long, value_parser = COMPONENTS)]
    pub component: Option<String>,

    /// Emit compact JSON instead of pretty-printed JSON.
    #[arg(long)]
    pub compact: bool,
}

/// Generate JSON Schema for declarative component documents.
#[derive(Debug, Clone, Args)]
pub struct SchemaArgs {
    /// Limit the schema to one component.
    #[arg(long, value_parser = COMPONENTS)]
    pub component: Option<String>,

    /// Emit compact JSON instead of pretty-printed JSON.
    #[arg(long)]
    pub compact: bool,
}

/// Execute an interactive component from a declarative JSON document.
#[derive(Debug, Clone, Args)]
pub struct RunArgs {
    /// JSON document path, or - to read the document from stdin.
    #[arg(value_name = "FILE", default_value = "-")]
    pub file: PathBuf,

    /// Print the resolved argv as JSON without executing the component.
    #[arg(long)]
    pub dry_run: bool,
}

/// Complete serializable registry response.
#[derive(Debug, Clone, Serialize)]
pub struct RegistryDocument {
    pub version: u32,
    pub components: Vec<ComponentMetadata>,
}

/// One command and every argument exposed by its Clap definition.
#[derive(Debug, Clone, Serialize)]
pub struct ComponentMetadata {
    pub name: String,
    pub about: String,
    pub arguments: Vec<ArgumentMetadata>,
}

/// Stable argument metadata for generators, editors, and external SDKs.
#[derive(Debug, Clone, Serialize)]
pub struct ArgumentMetadata {
    pub id: String,
    pub flag: Option<String>,
    pub aliases: Vec<String>,
    pub positional: bool,
    pub required: bool,
    pub multiple: bool,
    pub value_type: ValueType,
    pub possible_values: Vec<String>,
    pub defaults: Vec<String>,
    pub help: String,
}

/// JSON-facing primitive kind inferred from the executable argument contract.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ValueType {
    Boolean,
    Integer,
    String,
}

/// Versioned JSON document accepted by `rodtui run`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeclarativeDocument {
    pub version: u32,
    pub component: String,
    #[serde(default)]
    pub args: Map<String, Value>,
}

/// Build metadata by inspecting the real Clap command tree.
pub fn registry(component: Option<&str>) -> io::Result<RegistryDocument> {
    let root = Cli::command();
    let names: Vec<&str> = match component {
        Some(name) => vec![name],
        None => COMPONENTS.to_vec(),
    };
    let components = names
        .into_iter()
        .map(|name| {
            root.find_subcommand(name)
                .map(component_metadata)
                .ok_or_else(|| invalid(format!("unknown component {name:?}")))
        })
        .collect::<io::Result<Vec<_>>>()?;
    Ok(RegistryDocument {
        version: REGISTRY_VERSION,
        components,
    })
}

/// Serialize registry metadata with stable key ordering.
pub fn registry_json(args: &RegistryArgs) -> io::Result<Vec<u8>> {
    to_json(&registry(args.component.as_deref())?, args.compact)
}

/// Generate a JSON Schema whose properties come from the executable registry.
pub fn schema_json(args: &SchemaArgs) -> io::Result<Vec<u8>> {
    let registry = registry(args.component.as_deref())?;
    let variants: Vec<Value> = registry.components.iter().map(component_schema).collect();
    let schema = json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://rodtui.dev/schema/v1/component.json",
        "title": "rodtui declarative component",
        "description": "A versioned rodtui component invocation generated from the CLI registry.",
        "oneOf": variants
    });
    to_json(&schema, args.compact)
}

/// Read, validate, and translate a JSON document into the normal CLI command.
pub fn resolve_run(args: &RunArgs) -> io::Result<(Commands, Vec<String>)> {
    let bytes = read_document(&args.file)?;
    let document: DeclarativeDocument = serde_json::from_slice(&bytes)
        .map_err(|error| invalid(format!("invalid declarative JSON: {error}")))?;
    resolve_document(&document)
}

/// Resolve an in-memory document for the flow engine and other adapters.
pub fn resolve_document(document: &DeclarativeDocument) -> io::Result<(Commands, Vec<String>)> {
    let argv = document_argv(document)?;
    let cli = Cli::try_parse_from(&argv)
        .map_err(|error| invalid(format!("invalid declarative component: {error}")))?;
    if matches!(
        &cli.command,
        Commands::Choose(_) | Commands::Confirm(_) | Commands::Input(_)
    ) {
        Ok((cli.command, argv))
    } else {
        Err(invalid(
            "declarative execution only supports interactive components",
        ))
    }
}

/// Read a JSON document source using the same file/stdin contract as `run`.
pub fn read_source(path: &PathBuf) -> io::Result<Vec<u8>> {
    read_document(path)
}

/// Convert a document to argv while rejecting unknown or structurally invalid keys.
pub fn document_argv(document: &DeclarativeDocument) -> io::Result<Vec<String>> {
    if document.version != REGISTRY_VERSION {
        return Err(invalid(format!(
            "unsupported document version {}; expected {REGISTRY_VERSION}",
            document.version
        )));
    }
    if !COMPONENTS.contains(&document.component.as_str()) {
        return Err(invalid(format!(
            "unknown declarative component {:?}; expected choose, confirm, or input",
            document.component
        )));
    }

    let root = Cli::command();
    let command = root
        .find_subcommand(&document.component)
        .expect("component allowlist is synchronized with Clap");
    let mut positional = BTreeMap::<usize, Vec<String>>::new();
    let mut options = Vec::new();

    for (key, value) in &document.args {
        let argument = find_argument(command, key).ok_or_else(|| {
            invalid(format!(
                "unknown {} property {key:?}; run `rodtui registry --component {}` for supported properties",
                document.component, document.component
            ))
        })?;
        let values = json_values(key, value)?;
        if argument.get_long().is_none() {
            let index = argument.get_index().unwrap_or(1);
            positional.entry(index).or_default().extend(values);
            continue;
        }
        let flag = argument
            .get_long()
            .ok_or_else(|| invalid(format!("property {key:?} has no declarative flag")))?;
        match argument.get_action() {
            ArgAction::SetTrue => {
                let enabled = value
                    .as_bool()
                    .ok_or_else(|| invalid(format!("property {key:?} must be a boolean")))?;
                if enabled {
                    options.push(format!("--{flag}"));
                }
            }
            ArgAction::SetFalse => {
                let enabled = value
                    .as_bool()
                    .ok_or_else(|| invalid(format!("property {key:?} must be a boolean")))?;
                if !enabled {
                    options.push(format!("--{flag}"));
                }
            }
            _ => {
                for value in values {
                    options.push(format!("--{flag}"));
                    options.push(value);
                }
            }
        }
    }

    let mut argv = vec!["rodtui".to_string(), document.component.clone()];
    for values in positional.into_values() {
        argv.extend(values);
    }
    argv.extend(options);
    Ok(argv)
}

fn component_metadata(command: &Command) -> ComponentMetadata {
    ComponentMetadata {
        name: command.get_name().to_string(),
        about: command
            .get_about()
            .map(ToString::to_string)
            .unwrap_or_default(),
        arguments: command.get_arguments().map(argument_metadata).collect(),
    }
}

fn argument_metadata(argument: &Arg) -> ArgumentMetadata {
    let id = argument.get_id().to_string();
    let possible_values = argument
        .get_value_parser()
        .possible_values()
        .map(|values| values.map(|value| value.get_name().to_string()).collect())
        .unwrap_or_default();
    ArgumentMetadata {
        flag: argument.get_long().map(ToOwned::to_owned),
        aliases: semantic_aliases(&id),
        positional: argument.get_long().is_none(),
        required: argument.is_required_set(),
        multiple: matches!(argument.get_action(), ArgAction::Append)
            || matches!(id.as_str(), "items" | "selected" | "input_file"),
        value_type: value_type(argument),
        possible_values,
        defaults: argument
            .get_default_values()
            .iter()
            .map(|value| value.to_string_lossy().into_owned())
            .collect(),
        help: argument
            .get_help()
            .map(ToString::to_string)
            .unwrap_or_default(),
        id,
    }
}

fn value_type(argument: &Arg) -> ValueType {
    if matches!(
        argument.get_action(),
        ArgAction::SetTrue | ArgAction::SetFalse
    ) {
        ValueType::Boolean
    } else if matches!(
        argument.get_id().as_str(),
        "initial"
            | "limit"
            | "selected"
            | "width"
            | "height"
            | "padding"
            | "min_length"
            | "max_length"
            | "char_limit"
    ) {
        ValueType::Integer
    } else {
        ValueType::String
    }
}

fn semantic_aliases(id: &str) -> Vec<String> {
    let prefix = match id {
        "width" | "height" | "position" | "vertical_position" | "fullscreen" => "viewport",
        "border" | "border_color" | "title_color" | "title_alignment" | "padding" => "block",
        "foreground" | "background" | "selected_fg" | "selected_bg" | "instruction_fg"
        | "theme" => "style",
        "instruction_mode" => "instruction",
        "case" => "search",
        _ => return Vec::new(),
    };
    vec![format!("{prefix}.{}", id.replace('_', "-"))]
}

fn component_schema(component: &ComponentMetadata) -> Value {
    let mut properties = Map::new();
    for argument in &component.arguments {
        let mut property = Map::new();
        let primitive = match argument.value_type {
            ValueType::Boolean => "boolean",
            ValueType::Integer => "integer",
            ValueType::String => "string",
        };
        if argument.multiple {
            property.insert("type".into(), json!("array"));
            property.insert("items".into(), json!({ "type": primitive }));
        } else {
            property.insert("type".into(), json!(primitive));
        }
        if !argument.help.is_empty() {
            property.insert("description".into(), json!(argument.help));
        }
        if !argument.possible_values.is_empty() {
            property.insert("enum".into(), json!(argument.possible_values));
        }
        if argument.defaults.len() == 1 {
            property.insert(
                "default".into(),
                default_json(argument.value_type, &argument.defaults[0]),
            );
        }
        properties.insert(argument.id.clone(), Value::Object(property.clone()));
        for alias in &argument.aliases {
            properties.insert(alias.clone(), Value::Object(property.clone()));
        }
    }
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["version", "component"],
        "properties": {
            "version": { "const": REGISTRY_VERSION },
            "component": { "const": component.name },
            "args": {
                "type": "object",
                "additionalProperties": false,
                "properties": properties
            }
        }
    })
}

fn default_json(value_type: ValueType, value: &str) -> Value {
    match value_type {
        ValueType::Boolean => value
            .parse::<bool>()
            .map(Value::Bool)
            .unwrap_or(Value::Null),
        ValueType::Integer => value
            .parse::<u64>()
            .map(|value| Value::Number(value.into()))
            .unwrap_or(Value::Null),
        ValueType::String => Value::String(value.to_string()),
    }
}

fn find_argument<'a>(command: &'a Command, key: &str) -> Option<&'a Arg> {
    let normalized = key.replace('-', "_");
    let tail = key.rsplit('.').next().unwrap_or(key).replace('-', "_");
    command.get_arguments().find(|argument| {
        let id = argument.get_id().as_str();
        id == normalized || id == tail || argument.get_long().is_some_and(|long| long == key)
    })
}

fn json_values(key: &str, value: &Value) -> io::Result<Vec<String>> {
    match value {
        Value::Null => Ok(Vec::new()),
        Value::String(value) => Ok(vec![value.clone()]),
        Value::Bool(value) => Ok(vec![value.to_string()]),
        Value::Number(value) => Ok(vec![value.to_string()]),
        Value::Array(values) => values
            .iter()
            .map(|value| match value {
                Value::String(value) => Ok(value.clone()),
                Value::Bool(value) => Ok(value.to_string()),
                Value::Number(value) => Ok(value.to_string()),
                _ => Err(invalid(format!(
                    "property {key:?} arrays may contain only strings, numbers, or booleans"
                ))),
            })
            .collect(),
        Value::Object(_) => Err(invalid(format!("property {key:?} must not be an object"))),
    }
}

fn read_document(path: &PathBuf) -> io::Result<Vec<u8>> {
    if path.as_os_str() == "-" {
        if io::stdin().is_terminal() {
            return Err(invalid("rodtui run - requires a JSON document on stdin"));
        }
        let mut bytes = Vec::new();
        io::stdin().read_to_end(&mut bytes)?;
        Ok(bytes)
    } else {
        fs::read(path).map_err(|error| {
            io::Error::new(
                error.kind(),
                format!(
                    "could not read declarative file {}: {error}",
                    path.display()
                ),
            )
        })
    }
}

fn to_json(value: &impl Serialize, compact: bool) -> io::Result<Vec<u8>> {
    let mut bytes = if compact {
        serde_json::to_vec(value)
    } else {
        serde_json::to_vec_pretty(value)
    }
    .map_err(io::Error::other)?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Map};

    use super::{document_argv, registry, DeclarativeDocument, ValueType};

    #[test]
    fn registry_discovers_cli_defaults_enums_and_semantic_aliases() {
        let registry = registry(Some("choose")).expect("registry");
        let choose = &registry.components[0];
        let theme = choose
            .arguments
            .iter()
            .find(|argument| argument.id == "theme")
            .expect("theme argument");
        assert_eq!(theme.value_type, ValueType::String);
        assert!(theme.possible_values.contains(&"dracula".into()));
        assert_eq!(theme.defaults, ["rod"]);
        assert!(theme.aliases.contains(&"style.theme".into()));
    }

    #[test]
    fn declarative_choose_translates_arrays_aliases_and_booleans() {
        let document = DeclarativeDocument {
            version: 1,
            component: "choose".into(),
            args: serde_json::from_value(json!({
                "items": ["Deploy", "Build", "Test"],
                "filter": true,
                "query": "bld",
                "select_1": true,
                "style.theme": "dracula",
                "output": "json"
            }))
            .expect("args map"),
        };
        let argv = document_argv(&document).expect("argv");
        assert_eq!(&argv[0..5], ["rodtui", "choose", "Deploy", "Build", "Test"]);
        assert!(argv.windows(2).any(|pair| pair == ["--query", "bld"]));
        assert!(argv.contains(&"--filter".into()));
        assert!(argv.contains(&"--select-1".into()));
    }

    #[test]
    fn declarative_documents_reject_unknown_properties_and_versions() {
        let unknown = DeclarativeDocument {
            version: 1,
            component: "input".into(),
            args: serde_json::from_value(json!({ "monaco": true })).expect("args map"),
        };
        assert!(document_argv(&unknown)
            .expect_err("unknown property")
            .to_string()
            .contains("unknown input property"));

        let version = DeclarativeDocument {
            version: 99,
            component: "confirm".into(),
            args: Map::new(),
        };
        assert!(document_argv(&version)
            .expect_err("version")
            .to_string()
            .contains("unsupported document version"));
    }
}
