//! rodtui: a small executable entry point for a reusable terminal UI toolkit.
//!
//! This first milestone implements a real, interactive `choose` command.
//! Future milestones will move its reusable logic into the public SDK.

mod choose;
mod registry;

use clap::{Parser, Subcommand};
use registry::ChooseArgs;
use std::io;

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
    /// Choose one value from a list using arrow keys or j/k.
    Choose(ChooseArgs),
}

fn main() {
    // Keep errors on stderr so stdout remains suitable for shell pipelines.
    if let Err(error) = run() {
        eprintln!("rodtui: {error}");
        std::process::exit(1);
    }
}

/// Parse the command, run it, and print only the chosen value to stdout.
fn run() -> io::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Choose(args) => {
            // A real terminal is required because the user navigates interactively.
            let selection = choose::choose(&args)?;
            match selection {
                Some(index) => println!("{}", args.items[index]),
                None => std::process::exit(130),
            }
        }
    }
    Ok(())
}
