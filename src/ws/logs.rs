// Dormant under API-key auth: the log-stream WebSocket is JWT-only. Kept for future rewiring.
#![allow(dead_code)]
use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tokio_tungstenite::{connect_async, tungstenite::Message};

pub async fn stream(ws_url: &str, token: &str) -> Result<()> {
    let (mut ws, _) = connect_async(ws_url).await?;

    ws.send(Message::Text(
        serde_json::json!({ "type": "auth", "token": token }).to_string(),
    ))
    .await?;

    loop {
        tokio::select! {
            msg = ws.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(val) = serde_json::from_str::<Value>(&text) {
                            if val["type"] == "log" || val["type"] == "message" {
                                let line = val["data"]
                                    .as_str()
                                    .or_else(|| val["message"].as_str())
                                    .unwrap_or(text.as_str());
                                print!("{}", line);
                            } else if val["type"] == "error" {
                                eprintln!("Error: {}", val["message"].as_str().unwrap_or("unknown"));
                                break;
                            }
                        } else {
                            print!("{}", text);
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
            _ = tokio::signal::ctrl_c() => {
                let _ = ws.send(Message::Close(None)).await;
                break;
            }
        }
    }

    Ok(())
}
