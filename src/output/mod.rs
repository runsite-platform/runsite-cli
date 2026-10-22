pub mod format;

use crate::cli::OutputFormat;
use anyhow::Result;
use serde::Serialize;
use std::io::{self, Write};
use tabled::{Table, Tabled};

/// What a CLI command returns: rendered as JSON with `--output json`, or as
/// human-readable text otherwise. Commands never print to stdout themselves.
pub trait Report: Serialize {
    fn write_text(&self, out: &mut dyn Write) -> io::Result<()>;
}

/// Print `report` to stdout in the chosen format.
pub fn emit<R: Report>(format: OutputFormat, report: &R) -> Result<()> {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    write_report(format, report, &mut out)
}

pub fn write_report<R: Report>(
    format: OutputFormat,
    report: &R,
    out: &mut dyn Write,
) -> Result<()> {
    match format {
        OutputFormat::Json => {
            serde_json::to_writer_pretty(&mut *out, report)?;
            writeln!(out)?;
        }
        OutputFormat::Text => report.write_text(out)?,
    }
    Ok(())
}

pub fn write_table<T: Tabled>(out: &mut dyn Write, rows: Vec<T>) -> io::Result<()> {
    if rows.is_empty() {
        return writeln!(out, "No items found.");
    }
    writeln!(out, "{}", Table::new(rows))
}

#[cfg(test)]
pub fn render_text<R: Report>(report: &R) -> String {
    let mut buffer = Vec::new();
    write_report(OutputFormat::Text, report, &mut buffer).unwrap();
    String::from_utf8(buffer).unwrap()
}

#[cfg(test)]
pub fn render_json<R: Report>(report: &R) -> serde_json::Value {
    let mut buffer = Vec::new();
    write_report(OutputFormat::Json, report, &mut buffer).unwrap();
    serde_json::from_slice(&buffer).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Serialize)]
    struct Greeting {
        name: String,
    }

    impl Report for Greeting {
        fn write_text(&self, out: &mut dyn Write) -> io::Result<()> {
            writeln!(out, "Hello, {}", self.name)
        }
    }

    #[test]
    fn json_mode_prints_only_the_serialized_report() {
        let greeting = Greeting {
            name: "runsite".to_string(),
        };
        assert_eq!(
            render_json(&greeting),
            serde_json::json!({"name": "runsite"})
        );
    }

    #[test]
    fn text_mode_uses_the_report_text() {
        let greeting = Greeting {
            name: "runsite".to_string(),
        };
        assert_eq!(render_text(&greeting), "Hello, runsite\n");
    }

    #[test]
    fn an_empty_table_says_so() {
        #[derive(Tabled)]
        struct Row {
            name: String,
        }
        let mut buffer = Vec::new();
        write_table(&mut buffer, Vec::<Row>::new()).unwrap();
        assert_eq!(String::from_utf8(buffer).unwrap(), "No items found.\n");
    }
}
