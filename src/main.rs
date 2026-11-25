//! rodtui: a small executable entry point for a reusable terminal UI toolkit.
//!
//! This first milestone implements a real, interactive `choose` command.
//! Future milestones will move its reusable logic into the public SDK.

mod catalog;
mod choice;
mod choose;
mod form;
mod flow;
mod input;
mod output;
mod registry;
mod terminal_backend;
mod terminal_session;

use catalog::{RegistryArgs, RunArgs, SchemaArgs};
use clap::{Parser, Subcommand};
use flow::FlowArgs;
use registry::{ChooseArgs, ConfirmArgs, ConfirmOutput, InputArgs, InputOutput};
use std::io::{self, Write};

/// Clap turns these Rust types into validated CLI arguments and generated help.
#[derive(Debug, Parser)]
#[command(
    name = "rodtui",
    version,
    about = "🦀 Compose delightful terminal interfaces"
)]
struct Cli {
    /// Select which rodtui command to execute.
    #[command(subcommand)]
    command: Commands,
}

/// Each enum variant represents one public subcommand.
#[derive(Debug, Subcommand)]
enum Commands {
    /// Choose one or more values from arguments, files, or piped stdin.
    Choose(ChooseArgs),
    /// Ask a themed yes/no question with shell-friendly exit codes.
    Confirm(ConfirmArgs),
    /// Edit and validate one line of text.
    Input(InputArgs),
    /// Describe components and arguments as machine-readable JSON.
    Registry(RegistryArgs),
    /// Generate JSON Schema for declarative component documents.
    Schema(SchemaArgs),
    /// Execute choose, confirm, or input from a JSON document.
    Run(RunArgs),
    /// Execute a conditional multi-component JSON flow.
    Flow(FlowArgs),
}

fn main() {
    // Keep diagnostics on stderr and reserve stdout for selected data.
    let exit_code = match run() {
        Ok(exit_code) => exit_code,
        Err(error) => {
            eprintln!("rodtui: {error}");
            2
        }
    };
    if exit_code != 0 {
        std::process::exit(exit_code);
    }
}

/// Parse the command and return its documented process exit code.
fn run() -> io::Result<i32> {
    let cli = Cli::parse();
    execute(cli.command)
}

/// Execute an already parsed command, including declarative commands resolved at runtime.
fn execute(command: Commands) -> io::Result<i32> {
    match command {
        Commands::Choose(args) => {
            let items = input::collect_items(&args)?;
            if items.is_empty() {
                return Ok(1);
            }
            let state = choice::ChoiceState::new(items, &args)
                .map_err(|message| io::Error::new(io::ErrorKind::InvalidInput, message))?;
            match choose::choose(&args, state)? {
                choose::ChooseResult::Accepted(outcome) => {
                    let stdout = io::stdout();
                    let mut writer = stdout.lock();
                    match output::write_outcome(&mut writer, &args, &outcome) {
                        Ok(()) => writer.flush()?,
                        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => return Ok(0),
                        Err(error) => return Err(error),
                    }
                    Ok(0)
                }
                choose::ChooseResult::NoMatch => Ok(1),
                choose::ChooseResult::Cancelled => Ok(130),
            }
        }
        Commands::Confirm(args) => match form::confirm(&args)? {
            form::FormResult::Accepted(value) => {
                let rendered = match args.output {
                    ConfirmOutput::None => None,
                    ConfirmOutput::Text => Some(format!("{value}\n")),
                    ConfirmOutput::Json => {
                        Some(format!("{}\n", serde_json::json!({ "confirmed": value })))
                    }
                };
                if let Some(rendered) = rendered {
                    write_stdout(rendered.as_bytes())?;
                }
                Ok(if value { 0 } else { 1 })
            }
            form::FormResult::Cancelled => Ok(130),
        },
        Commands::Input(args) => {
            let initial = form::initial_input(&args)?;
            match form::input(&args, initial)? {
                form::FormResult::Accepted(value) => {
                    let rendered = match args.output {
                        InputOutput::Text => format!("{value}\n"),
                        InputOutput::Json => {
                            format!("{}\n", serde_json::json!({ "value": value }))
                        }
                    };
                    write_stdout(rendered.as_bytes())?;
                    Ok(0)
                }
                form::FormResult::Cancelled => Ok(130),
            }
        }
        Commands::Registry(args) => {
            write_stdout(&catalog::registry_json(&args)?)?;
            Ok(0)
        }
        Commands::Schema(args) => {
            write_stdout(&catalog::schema_json(&args)?)?;
            Ok(0)
        }
        Commands::Run(args) => {
            let (command, argv) = catalog::resolve_run(&args)?;
            if args.dry_run {
                let rendered = serde_json::to_vec_pretty(&argv).map_err(io::Error::other)?;
                write_stdout(&rendered)?;
                write_stdout(b"\n")?;
                Ok(0)
            } else {
                execute(command)
            }
        }
        Commands::Flow(args) => {
            let execution = flow::run(&args)?;
            write_stdout(&flow::report_json(&execution, args.compact)?)?;
            Ok(execution.exit_code)
        }
    }
}

/// Write a machine-readable result while treating a closed pipeline as success.
fn write_stdout(bytes: &[u8]) -> io::Result<()> {
    let stdout = io::stdout();
    let mut writer = stdout.lock();
    match writer.write_all(bytes).and_then(|()| writer.flush()) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        Err(error) => Err(error),
    }
}
