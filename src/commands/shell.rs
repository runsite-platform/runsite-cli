use crate::api::ApiClient;
use anyhow::Result;

pub async fn interactive(_client: &ApiClient, _service: Option<&str>) -> Result<()> {
    // Gated off under API-key auth: the shell WebSocket is JWT-only.
    println!(
        "`runsite shell` is not available with API-key authentication yet. \
         Use the dashboard to open a shell."
    );
    Ok(())
}

pub async fn run(_client: &ApiClient, _service: Option<&str>, _command: &[String]) -> Result<()> {
    // Gated off under API-key auth: the shell WebSocket is JWT-only.
    println!(
        "`runsite run` is not available with API-key authentication yet. \
         Use the dashboard to run a command."
    );
    Ok(())
}
