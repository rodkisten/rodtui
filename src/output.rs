//! Stable stdout encoders for selected values.
//!
//! UI rendering remains on stderr. These encoders are intentionally small and
//! deterministic so shell scripts can depend on text, index, or JSON output.

use std::io::{self, Write};

use serde::Serialize;

use crate::{
    choice::{Choice, ChoiceOutcome},
    registry::{ChooseArgs, OutputFormat},
};

#[derive(Serialize)]
struct JsonOutcome<'a> {
    query: &'a str,
    selected: Vec<JsonChoice<'a>>,
}

#[derive(Serialize)]
struct JsonChoice<'a> {
    index: usize,
    value: &'a str,
}

/// Write an accepted result using the selected machine-readable contract.
pub fn write_outcome(
    writer: &mut impl Write,
    args: &ChooseArgs,
    outcome: &ChoiceOutcome,
) -> io::Result<()> {
    let terminator = if args.print0 { b'\0' } else { b'\n' };
    match args.output {
        OutputFormat::Text => write_records(writer, &outcome.selected, terminator, |choice| {
            choice.value.as_bytes().to_vec()
        }),
        OutputFormat::Index => write_records(writer, &outcome.selected, terminator, |choice| {
            choice.original_index.to_string().into_bytes()
        }),
        OutputFormat::Json => {
            let document = JsonOutcome {
                query: &outcome.query,
                selected: outcome
                    .selected
                    .iter()
                    .map(|choice| JsonChoice {
                        index: choice.original_index,
                        value: &choice.value,
                    })
                    .collect(),
            };
            serde_json::to_writer(&mut *writer, &document).map_err(io::Error::other)?;
            writer.write_all(&[terminator])
        }
    }
}

fn write_records(
    writer: &mut impl Write,
    choices: &[Choice],
    terminator: u8,
    encode: impl Fn(&Choice) -> Vec<u8>,
) -> io::Result<()> {
    for choice in choices {
        writer.write_all(&encode(choice))?;
        writer.write_all(&[terminator])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::write_outcome;
    use crate::{
        choice::{Choice, ChoiceOutcome},
        Cli, Commands,
    };

    #[test]
    fn json_output_contains_query_indices_and_values() {
        let cli = Cli::try_parse_from(["rodtui", "choose", "--output", "json"]).expect("arguments");
        let Commands::Choose(args) = cli.command else {
            panic!("expected choose command")
        };
        let outcome = ChoiceOutcome {
            query: "bu".into(),
            selected: vec![Choice {
                original_index: 1,
                value: "Build".into(),
            }],
        };
        let mut bytes = Vec::new();
        write_outcome(&mut bytes, &args, &outcome).expect("output");
        let json: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(json["query"], "bu");
        assert_eq!(json["selected"][0]["index"], 1);
        assert_eq!(json["selected"][0]["value"], "Build");
    }
}
