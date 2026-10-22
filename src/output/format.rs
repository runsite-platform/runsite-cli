use chrono::{DateTime, Utc};
use colored::Colorize;

use crate::api::{Condition, Status};

pub fn colorize_status(status: &impl Status) -> String {
    let text = status.to_string();
    match status.condition() {
        Condition::Running => text.green().to_string(),
        Condition::Stopped | Condition::Sleeping | Condition::Deleting => text.dimmed().to_string(),
        Condition::Busy => text.yellow().to_string(),
        Condition::Failed => text.red().bold().to_string(),
        Condition::Blocked => text.red().to_string(),
        Condition::Unknown => text,
    }
}

/// "3s ago", "2m ago", "2h ago", "3d ago". A future `then` counts as now.
pub fn format_age(now: DateTime<Utc>, then: DateTime<Utc>) -> String {
    let seconds = (now - then).num_seconds().max(0);
    match seconds {
        0..=59 => format!("{seconds}s ago"),
        60..=3599 => format!("{}m ago", seconds / 60),
        3600..=86_399 => format!("{}h ago", seconds / 3600),
        _ => format!("{}d ago", seconds / 86_400),
    }
}

pub fn format_time(time: &Option<DateTime<Utc>>) -> String {
    time.map(|then| format_age(Utc::now(), then))
        .unwrap_or_else(|| PLACEHOLDER.to_string())
}

/// Shown in text output where a value is missing.
pub const PLACEHOLDER: &str = "—";

pub fn or_placeholder(value: Option<&str>) -> String {
    value.unwrap_or(PLACEHOLDER).to_string()
}

/// The first 8 characters of an ID, as `deployments list` prints it.
pub fn short_id(id: &uuid::Uuid) -> String {
    id.to_string()[..8].to_string()
}
