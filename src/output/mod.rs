pub mod format;

use crate::cli::OutputFormat;
use anyhow::Result;
use serde::Serialize;
use tabled::{Table, Tabled};

pub fn print_table<T: Tabled>(rows: Vec<T>) {
    if rows.is_empty() {
        println!("No items found.");
        return;
    }
    println!("{}", Table::new(rows));
}

pub fn print_json<T: Serialize>(value: &T) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

/// Render `value` as JSON, or fall back to the caller's text rendering.
pub fn render<T: Serialize>(format: OutputFormat, value: &T, as_text: impl FnOnce()) -> Result<()> {
    match format {
        OutputFormat::Json => print_json(value),
        OutputFormat::Text => {
            as_text();
            Ok(())
        }
    }
}
