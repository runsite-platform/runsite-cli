pub mod auth;
pub mod context;
pub mod database;
pub mod deploy;
pub mod deployments;
pub mod env;
pub mod logs;
pub mod project;
pub mod service;
pub mod shell;

use anyhow::{anyhow, Result};

/// Splits `KEY=VALUE` at the first `=`, so the value may contain `=` itself.
pub fn parse_key_value(raw: &str) -> Result<(&str, &str)> {
    raw.split_once('=')
        .ok_or_else(|| anyhow!("invalid '{}', expected KEY=VALUE", raw))
}
