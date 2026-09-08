// Dormant under API-key auth: the shell WebSocket is JWT-only. Kept for future rewiring.
#![allow(dead_code)]
use anyhow::Result;
use crossterm::terminal;
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use std::io::Write;
use tokio::io::AsyncReadExt;
use tokio_tungstenite::{connect_async, tungstenite::Message};

pub async fn run_interactive(ws_url: &str, token: &str) -> Result<()> {
    let (mut ws, _) = connect_async(ws_url).await?;

    ws.send(Message::Text(
        serde_json::json!({ "type": "auth", "token": token }).to_string(),
    ))
    .await?;

    // Wait for "connected" message
    while let Some(msg) = ws.next().await {
        if let Ok(Message::Text(text)) = msg {
            if let Ok(val) = serde_json::from_str::<Value>(&text) {
                if val["type"] == "connected" {
                    break;
                }
                if val["type"] == "error" {
                    return Err(anyhow::anyhow!(
                        "{}",
                        val["message"].as_str().unwrap_or("connection failed")
                    ));
                }
            }
        }
    }

    // Send initial terminal size
    let (cols, rows) = terminal::size()?;
    ws.send(Message::Text(
        serde_json::json!({ "type": "resize", "cols": cols, "rows": rows }).to_string(),
    ))
    .await?;

    terminal::enable_raw_mode()?;
    // Ensure raw mode is restored even on panic
    let _raw_guard = RawModeGuard;

    let (mut ws_write, mut ws_read) = ws.split();

    let stdin_task = tokio::spawn(async move {
        let mut stdin = tokio::io::stdin();
        let mut buf = [0u8; 256];
        loop {
            match stdin.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    let data = String::from_utf8_lossy(&buf[..n]).to_string();
                    let msg = serde_json::json!({ "type": "input", "data": data }).to_string();
                    if ws_write.send(Message::Text(msg)).await.is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    let stdout_task = tokio::spawn(async move {
        let mut stdout = std::io::stdout();
        while let Some(msg) = ws_read.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    if let Ok(val) = serde_json::from_str::<Value>(&text) {
                        match val["type"].as_str() {
                            Some("output") => {
                                if let Some(data) = val["data"].as_str() {
                                    let _ = stdout.write_all(data.as_bytes());
                                    let _ = stdout.flush();
                                }
                            }
                            Some("error") => {
                                let _ = terminal::disable_raw_mode();
                                eprintln!("\r\nError: {}", val["message"].as_str().unwrap_or(""));
                                break;
                            }
                            Some("pong") => {}
                            _ => {}
                        }
                    }
                }
                Ok(Message::Close(_)) | Err(_) => break,
                _ => {}
            }
        }
    });

    tokio::select! {
        _ = stdin_task => {}
        _ = stdout_task => {}
    }

    Ok(())
}

pub async fn run_command(ws_url: &str, token: &str, command: &[String]) -> Result<()> {
    let (mut ws, _) = connect_async(ws_url).await?;

    ws.send(Message::Text(
        serde_json::json!({ "type": "auth", "token": token }).to_string(),
    ))
    .await?;

    // Wait for connection
    while let Some(msg) = ws.next().await {
        if let Ok(Message::Text(text)) = msg {
            if let Ok(val) = serde_json::from_str::<Value>(&text) {
                if val["type"] == "connected" {
                    break;
                }
                if val["type"] == "error" {
                    return Err(anyhow::anyhow!(
                        "{}",
                        val["message"].as_str().unwrap_or("connection failed")
                    ));
                }
            }
        }
    }

    // Send the command as a single input
    let cmd = command.join(" ") + "\n";
    ws.send(Message::Text(
        serde_json::json!({ "type": "input", "data": cmd }).to_string(),
    ))
    .await?;

    // Print output until close
    let mut stdout = std::io::stdout();
    while let Some(msg) = ws.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                if let Ok(val) = serde_json::from_str::<Value>(&text) {
                    match val["type"].as_str() {
                        Some("output") => {
                            if let Some(data) = val["data"].as_str() {
                                stdout.write_all(data.as_bytes())?;
                                stdout.flush()?;
                            }
                        }
                        Some("error") => {
                            eprintln!("Error: {}", val["message"].as_str().unwrap_or(""));
                            break;
                        }
                        _ => {}
                    }
                }
            }
            Ok(Message::Close(_)) | Err(_) => break,
            _ => {}
        }
    }

    Ok(())
}

struct RawModeGuard;

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
    }
}
