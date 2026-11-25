//! Candidate input collection for `rodtui choose`.
//!
//! Positional arguments, repeatable files, and piped stdin normalize into one
//! ordered list. The renderer never needs to know where an item originated.

use std::{
    collections::HashSet,
    fs,
    io::{self, IsTerminal, Read},
};

use crate::registry::ChooseArgs;

/// Collect candidates from every configured source.
pub fn collect_items(args: &ChooseArgs) -> io::Result<Vec<String>> {
    let mut items = args.items.clone();
    let mut piped_stdin = false;

    for path in &args.input_file {
        let bytes = fs::read(path).map_err(|error| {
            io::Error::new(
                error.kind(),
                format!("could not read input file {}: {error}", path.display()),
            )
        })?;
        append_records(&mut items, &bytes, args.read0)?;
    }

    if !args.no_stdin && !io::stdin().is_terminal() {
        piped_stdin = true;
        let mut bytes = Vec::new();
        io::stdin().read_to_end(&mut bytes)?;
        append_records(&mut items, &bytes, args.read0)?;
    }

    if args.dedupe {
        let mut seen = HashSet::new();
        items.retain(|item| seen.insert(item.clone()));
    }

    if items.is_empty() && !piped_stdin {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "choose requires at least one item from arguments, files, or stdin",
        ));
    }
    Ok(items)
}

/// Split UTF-8 records without inventing an empty record after a final delimiter.
fn append_records(target: &mut Vec<String>, bytes: &[u8], read0: bool) -> io::Result<()> {
    if bytes.is_empty() {
        return Ok(());
    }
    let delimiter = if read0 { b'\0' } else { b'\n' };
    let ends_with_delimiter = bytes.last() == Some(&delimiter);
    let mut records: Vec<&[u8]> = bytes.split(|byte| *byte == delimiter).collect();
    if ends_with_delimiter {
        records.pop();
    }

    for record in records {
        let record = if !read0 && record.last() == Some(&b'\r') {
            &record[..record.len() - 1]
        } else {
            record
        };
        let value = std::str::from_utf8(record).map_err(|error| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("choose input must be valid UTF-8: {error}"),
            )
        })?;
        target.push(value.to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::append_records;

    #[test]
    fn splits_lines_crlf_and_null_records() {
        let mut lines = Vec::new();
        append_records(&mut lines, b"alpha\r\nbeta\n", false).expect("lines");
        assert_eq!(lines, ["alpha", "beta"]);

        let mut nulls = Vec::new();
        append_records(&mut nulls, b"one\0two\0", true).expect("nulls");
        assert_eq!(nulls, ["one", "two"]);
    }
}
