use colored::Colorize;

pub fn colorize_status(status: &str) -> String {
    match status.to_lowercase().as_str() {
        "running" => status.green().to_string(),
        "stopped" | "stopping" => status.red().to_string(),
        "sleeping" => status.cyan().to_string(),
        "deploying" | "building" | "starting" => status.yellow().to_string(),
        "failed" | "error" | "crashed" => status.red().bold().to_string(),
        "succeeded" | "success" | "completed" => status.green().to_string(),
        "cancelled" | "canceled" => status.dimmed().to_string(),
        _ => status.normal().to_string(),
    }
}

pub fn format_time(dt: &Option<chrono::DateTime<chrono::Utc>>) -> String {
    dt.map(|t| {
        let dur = chrono::Utc::now().signed_duration_since(t);
        if dur.num_seconds() < 60 {
            format!("{}s ago", dur.num_seconds())
        } else if dur.num_minutes() < 60 {
            format!("{}m ago", dur.num_minutes())
        } else if dur.num_hours() < 24 {
            format!("{}h ago", dur.num_hours())
        } else {
            format!("{}d ago", dur.num_days())
        }
    })
    .unwrap_or_else(|| "—".to_string())
}
