use anyhow::{bail, Result};

// Both are gated off under API-key auth: the shell WebSocket is JWT-only.
// They fail instead of printing a notice, so scripts don't mistake them for success.

pub fn interactive(_service: Option<&str>) -> Result<()> {
    bail!(
        "`runsite shell` is not available with API-key authentication yet. \
         Use the dashboard to open a shell."
    )
}

pub fn run(_service: Option<&str>, _command: &[String]) -> Result<()> {
    bail!(
        "`runsite run` is not available with API-key authentication yet. \
         Use the dashboard to run a command."
    )
}
