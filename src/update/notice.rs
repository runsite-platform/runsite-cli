//! Once a day, check GitHub in the background and mention a newer release on stderr.

use super::{http_client, is_newer, ReleaseSource, CURRENT_VERSION};
use colored::Colorize;
use serde::{Deserialize, Serialize};
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::task::JoinHandle;

const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
const CHECK_TIMEOUT: Duration = Duration::from_secs(3);
/// How long a finished command may wait for a check still in flight.
const FINISH_GRACE: Duration = Duration::from_secs(1);

#[derive(Debug, Default, Serialize, Deserialize, PartialEq)]
struct CheckState {
    last_checked: u64,
    latest_version: Option<String>,
}

pub struct UpdateNotifier {
    cached_latest: Option<String>,
    pending_check: Option<JoinHandle<Option<String>>>,
}

impl UpdateNotifier {
    pub fn disabled() -> Self {
        Self {
            cached_latest: None,
            pending_check: None,
        }
    }

    /// Read the last result and, when it is older than a day, start a new check.
    pub fn start() -> Self {
        match state_path() {
            Some(path) => Self::start_at(path, ReleaseSource::github(), now_secs()),
            None => Self::disabled(),
        }
    }

    /// `start` with the cache file, the release source and the time given.
    fn start_at(path: PathBuf, source: ReleaseSource, now: u64) -> Self {
        let state = read_state(&path);
        let cached_latest = state.latest_version.clone();
        let pending_check = is_due(&state, now).then(|| {
            tokio::spawn(async move {
                let latest = check_latest(&source).await;
                // Recorded even on failure, so an offline machine does not retry every run.
                write_state(
                    &path,
                    &CheckState {
                        last_checked: now,
                        latest_version: latest.clone().or(state.latest_version),
                    },
                );
                latest
            })
        });
        Self {
            cached_latest,
            pending_check,
        }
    }

    /// Print the notice after the command has finished, so it never mixes with its output.
    pub async fn finish(self) {
        if let Some(message) = self.notice().await {
            eprintln!("\n{message}");
        }
    }

    /// The notice to print, if a newer version is known.
    async fn notice(self) -> Option<String> {
        let fresh = match self.pending_check {
            Some(handle) => tokio::time::timeout(FINISH_GRACE, handle)
                .await
                .ok()
                .and_then(Result::ok)
                .flatten(),
            None => None,
        };
        notice_text(&fresh.or(self.cached_latest)?, CURRENT_VERSION)
    }
}

/// Whether this run may print the notice: never in CI, scripts, JSON output or when opted out.
pub fn should_check(json_output: bool) -> bool {
    let opted_out = std::env::var_os("RUNSITE_NO_UPDATE_CHECK")
        .is_some_and(|value| !value.is_empty() && value != "0");
    let in_ci = std::env::var_os("CI").is_some_and(|value| !value.is_empty() && value != "false");
    !opted_out && !in_ci && !json_output && std::io::stderr().is_terminal()
}

/// Remember a result found by `runsite upgrade`, so the notice agrees with it.
pub fn record_check(latest_version: Option<&str>) {
    if let Some(path) = state_path() {
        write_state(
            &path,
            &CheckState {
                last_checked: now_secs(),
                latest_version: latest_version.map(str::to_string),
            },
        );
    }
}

fn notice_text(latest: &str, current: &str) -> Option<String> {
    is_newer(latest, current).then(|| {
        let latest = latest.trim_start_matches('v');
        format!(
            "{} runsite v{current} → v{latest}\nRun `{}` to update.",
            "Update available:".yellow(),
            "runsite upgrade".bold()
        )
    })
}

async fn check_latest(source: &ReleaseSource) -> Option<String> {
    let client = http_client(CHECK_TIMEOUT).ok()?;
    source
        .latest(&client)
        .await
        .ok()
        .map(|release| release.tag_name)
}

fn is_due(state: &CheckState, now: u64) -> bool {
    // A clock moved backwards also triggers a check instead of silencing it for good.
    now < state.last_checked || now - state.last_checked >= CHECK_INTERVAL.as_secs()
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

fn state_path() -> Option<PathBuf> {
    dirs::cache_dir().map(|dir| dir.join("runsite").join("update-check.json"))
}

fn read_state(path: &Path) -> CheckState {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok())
        .unwrap_or_default()
}

/// Best effort: a read-only cache directory just means checking again next time.
fn write_state(path: &Path, state: &CheckState) {
    let Some(dir) = path.parent() else { return };
    if std::fs::create_dir_all(dir).is_err() {
        return;
    }
    let Ok(content) = serde_json::to_string(state) else {
        return;
    };
    let staging = path.with_extension(format!("json.{}.tmp", std::process::id()));
    if std::fs::write(&staging, content).is_ok() && std::fs::rename(&staging, path).is_err() {
        let _ = std::fs::remove_file(&staging);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const DAY: u64 = 24 * 60 * 60;

    async fn serve_latest(server: &MockServer, tag: &str, expected_calls: u64) {
        Mock::given(method("GET"))
            .and(path("/repos/acme/cli/releases/latest"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"tag_name": tag})),
            )
            .expect(expected_calls)
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn a_due_check_asks_github_and_remembers_the_answer() {
        colored::control::set_override(false);
        let server = MockServer::start().await;
        serve_latest(&server, "v99.0.0", 1).await;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("update-check.json");
        let source = ReleaseSource::at(&server.uri(), "acme/cli");

        let notice = UpdateNotifier::start_at(path.clone(), source, 10 * DAY)
            .notice()
            .await
            .unwrap();
        assert!(notice.contains("→ v99.0.0"), "{notice}");
        assert_eq!(
            read_state(&path),
            CheckState {
                last_checked: 10 * DAY,
                latest_version: Some("v99.0.0".to_string()),
            }
        );
    }

    #[tokio::test]
    async fn a_recent_check_is_not_repeated_and_its_answer_is_reused() {
        colored::control::set_override(false);
        let server = MockServer::start().await;
        serve_latest(&server, "v99.0.0", 0).await;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("update-check.json");
        write_state(
            &path,
            &CheckState {
                last_checked: 10 * DAY,
                latest_version: Some("v98.0.0".to_string()),
            },
        );
        let source = ReleaseSource::at(&server.uri(), "acme/cli");

        let notice = UpdateNotifier::start_at(path, source, 10 * DAY + 60)
            .notice()
            .await
            .unwrap();
        assert!(notice.contains("→ v98.0.0"), "{notice}");
    }

    #[tokio::test]
    async fn a_failed_check_is_recorded_and_keeps_the_last_answer() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("update-check.json");
        write_state(
            &path,
            &CheckState {
                last_checked: DAY,
                latest_version: Some("v0.0.1".to_string()),
            },
        );
        let source = ReleaseSource::at(&server.uri(), "acme/cli");

        let notice = UpdateNotifier::start_at(path.clone(), source, 10 * DAY)
            .notice()
            .await;
        assert_eq!(notice, None, "an older cached version is not announced");
        assert_eq!(
            read_state(&path),
            CheckState {
                last_checked: 10 * DAY,
                latest_version: Some("v0.0.1".to_string()),
            }
        );
    }

    #[test]
    fn checks_at_most_once_a_day() {
        let day = CHECK_INTERVAL.as_secs();
        let state = CheckState {
            last_checked: 10 * day,
            latest_version: None,
        };
        assert!(!is_due(&state, 10 * day + day - 1));
        assert!(is_due(&state, 11 * day));
        assert!(is_due(&state, 9 * day));
        assert!(is_due(&CheckState::default(), 10 * day));
    }

    #[test]
    fn mentions_only_a_newer_version() {
        colored::control::set_override(false);
        assert_eq!(
            notice_text("v0.4.0", "0.3.0").as_deref(),
            Some("Update available: runsite v0.3.0 → v0.4.0\nRun `runsite upgrade` to update.")
        );
        assert_eq!(notice_text("v0.3.0", "0.3.0"), None);
        assert_eq!(notice_text("v0.2.0", "0.3.0"), None);
    }

    #[test]
    fn state_survives_a_round_trip_and_tolerates_garbage() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("update-check.json");
        assert_eq!(read_state(&path), CheckState::default());

        let state = CheckState {
            last_checked: 42,
            latest_version: Some("v1.2.3".to_string()),
        };
        write_state(&path, &state);
        assert_eq!(read_state(&path), state);

        std::fs::write(&path, "not json").unwrap();
        assert_eq!(read_state(&path), CheckState::default());
    }
}
