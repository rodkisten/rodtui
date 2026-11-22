//! rodtui: a small executable entry point for a reusable terminal UI toolkit.
//!
//! This first milestone implements a real, interactive `choose` command.
//! Future milestones will move its reusable logic into the public SDK.

mod choose;

use clap::{Parser, Subcommand};
use std::io;

/// Clap turns these Rust types into validated CLI arguments and generated help.
#[derive(Debug, Parser)]
#[command(name = "rodtui", version, about = "🦀 Compose delightful terminal interfaces")]
struct Cli {
    /// Select which rodtui command to execute.
    #[command(subcommand)]
    command: Commands,
}

/// Each enum variant represents one public subcommand.
#[derive(Debug, Subcommand)]
enum Commands {
    /// Choose one value from a list using arrow keys or j/k.
    Choose {
        /// Candidate values; quote an item if it contains spaces.
        #[arg(required = true)]
        items: Vec<String>,

        /// Optional heading shown above the choices.
        #[arg(long, short)]
        title: Option<String>,
    },
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
        Commands::Choose { items, title } => {
            // A real terminal is required because the user navigates interactively.
            let selection = choose::choose(&items, title.as_deref())?;
            match selection {
                Some(index) => println!("{}", items[index]),
                None => std::process::exit(130),
            }
        }
    }
    Ok(())
}
