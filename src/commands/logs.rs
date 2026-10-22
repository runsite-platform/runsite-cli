use crate::api::{ApiClient, ContainerLogs};
use crate::output::Report;
use anyhow::Result;
use serde::Serialize;
use std::io::{self, Write};

#[derive(Debug, Serialize)]
#[serde(transparent)]
pub struct Logs(pub ContainerLogs);

impl Report for Logs {
    fn write_text(&self, out: &mut dyn Write) -> io::Result<()> {
        let logs = &self.0.logs;
        if logs.trim().is_empty() {
            return writeln!(out, "No logs available.");
        }
        write!(out, "{}", logs)?;
        if !logs.ends_with('\n') {
            writeln!(out)?;
        }
        Ok(())
    }
}

pub async fn tail(client: &ApiClient, service: Option<&str>, lines: u32) -> Result<Logs> {
    let id = client.resolve_service_id(service).await?;
    Ok(Logs(ContainerLogs {
        logs: client.service_logs(id, lines, None).await?,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::render_text;

    fn logs(text: &str) -> Logs {
        Logs(ContainerLogs {
            logs: text.to_string(),
        })
    }

    #[test]
    fn logs_end_with_a_newline() {
        assert_eq!(render_text(&logs("started")), "started\n");
        assert_eq!(render_text(&logs("started\n")), "started\n");
    }

    #[test]
    fn blank_logs_say_so() {
        assert_eq!(render_text(&logs("  \n")), "No logs available.\n");
    }
}
