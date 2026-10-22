//! `Runtime` against a mock API and a temporary config file: effects go in,
//! the actions the worker reports come back on the channel.

use super::action::{Action, Effect, Mutation, Payload, Request};
use super::poller::Ticket;
use super::worker::Worker;
use super::Runtime;
use crate::api::test_support::*;
use crate::config::ConfigFile;
use crate::error::ApiError;
use std::path::Path;
use std::time::Duration;
use tokio::sync::mpsc::{self, UnboundedReceiver};
use wiremock::MockServer;

const STAGING_PROJECT: &str = "ca83c4a9-1517-4fe3-93aa-00000000000a";

fn user_json() -> serde_json::Value {
    serde_json::json!({
        "id": "8f14e45f-ceea-467a-9f0a-1c2d3e4f5a6b", "email": "ada@example.com",
        "name": "Ada", "active_workspace_name": null, "created_at": "2026-01-01T00:00:00Z"
    })
}

fn write_config(path: &Path, staging_url: &str) {
    let content = format!(
        r#"
current_profile = "default"

[profiles.default]
api_url = "http://127.0.0.1:9"

[profiles.staging]
api_url = "{staging_url}"
api_key = "{TEST_KEY}"
current_project_id = "{STAGING_PROJECT}"
"#
    );
    std::fs::write(path, content).unwrap();
}

/// A runtime whose worker talks to `server`, reading profiles from `config_path`.
fn runtime_for(server: &MockServer, config_path: &Path) -> (Runtime, UnboundedReceiver<Action>) {
    let (results, incoming) = mpsc::unbounded_channel();
    let (_, profile) = session_in_project(server, None);
    let runtime = Runtime {
        worker: Worker::new(profile, results.clone()),
        results,
        config: ConfigFile::at(config_path.to_path_buf(), None),
    };
    (runtime, incoming)
}

async fn next_action(incoming: &mut UnboundedReceiver<Action>) -> Action {
    tokio::time::timeout(Duration::from_secs(5), incoming.recv())
        .await
        .expect("the worker reported nothing within 5 seconds")
        .expect("the results channel closed")
}

#[tokio::test]
async fn a_fetch_comes_back_as_a_loaded_payload() {
    let server = MockServer::start().await;
    serve(&server, "/api/v1/users/me", user_json()).await;
    let dir = tempfile::tempdir().unwrap();
    let (mut runtime, mut incoming) = runtime_for(&server, &dir.path().join("config.toml"));

    assert!(runtime.execute(vec![Effect::Fetch {
        ticket: Ticket::new(7),
        request: Request::CurrentUser,
    }]));

    match next_action(&mut incoming).await {
        Action::Loaded {
            ticket,
            request: Request::CurrentUser,
            result: Ok(Payload::CurrentUser(user)),
        } => {
            assert_eq!(ticket, Ticket::new(7));
            assert_eq!(user.email, "ada@example.com");
        }
        _ => panic!("expected the current user"),
    }
}

#[tokio::test]
async fn an_api_error_comes_back_classified() {
    let server = MockServer::start().await;
    expect_request(
        &server,
        "GET",
        "/api/v1/users/me",
        403,
        serde_json::json!({"detail": "forbidden"}),
    )
    .await;
    let dir = tempfile::tempdir().unwrap();
    let (mut runtime, mut incoming) = runtime_for(&server, &dir.path().join("config.toml"));

    runtime.execute(vec![Effect::Fetch {
        ticket: Ticket::new(1),
        request: Request::CurrentUser,
    }]);

    assert!(matches!(
        next_action(&mut incoming).await,
        Action::Loaded {
            result: Err(ApiError::Forbidden { .. }),
            ..
        }
    ));
}

#[tokio::test]
async fn a_mutation_posts_to_its_route_and_reports_back() {
    let server = MockServer::start().await;
    expect_request(
        &server,
        "POST",
        &format!("/api/v1/web-services/{SERVICE_ID}/restart"),
        200,
        serde_json::json!({"status": "deploying", "container_id": null, "host_port": null}),
    )
    .await;
    let dir = tempfile::tempdir().unwrap();
    let (mut runtime, mut incoming) = runtime_for(&server, &dir.path().join("config.toml"));
    let mutation = Mutation::Restart {
        service_id: uuid(SERVICE_ID),
    };

    runtime.execute(vec![Effect::Mutate {
        generation: 2,
        mutation,
    }]);

    match next_action(&mut incoming).await {
        Action::Mutated {
            generation: 2,
            mutation: reported,
            result: Ok(()),
        } => assert_eq!(reported, mutation),
        _ => panic!("expected an accepted restart"),
    }
}

#[tokio::test]
async fn profiles_are_listed_from_the_config_file() {
    let server = MockServer::start().await;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    write_config(&path, &server.uri());
    let (mut runtime, mut incoming) = runtime_for(&server, &path);

    runtime.execute(vec![Effect::ListProfiles]);

    let Action::ProfilesListed(entries) = next_action(&mut incoming).await else {
        panic!("expected the profile list");
    };
    let names: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
    assert_eq!(names, ["default", "staging"]);
    assert!(!entries[0].has_key);
    assert_eq!(entries[1].current_project_id, Some(uuid(STAGING_PROJECT)));
}

#[tokio::test]
async fn after_a_switch_requests_go_to_the_new_profile() {
    let staging = MockServer::start().await;
    serve(&staging, "/api/v1/users/me", user_json()).await;
    let unused = MockServer::start().await;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    write_config(&path, &staging.uri());
    let (mut runtime, mut incoming) = runtime_for(&unused, &path);

    runtime.execute(vec![
        Effect::SwitchProfile {
            profile: "staging".to_string(),
        },
        Effect::Fetch {
            ticket: Ticket::new(3),
            request: Request::CurrentUser,
        },
    ]);

    assert!(matches!(
        next_action(&mut incoming).await,
        Action::Loaded {
            result: Ok(Payload::CurrentUser(_)),
            ..
        }
    ));
    assert!(unused.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn a_chosen_project_is_saved_to_its_profile() {
    let server = MockServer::start().await;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    write_config(&path, &server.uri());
    let config = ConfigFile::at(path, None);

    config.select_project("default", uuid(PROJECT_ID)).unwrap();

    let saved = config.read().unwrap();
    assert_eq!(
        saved.profiles["default"].selected_project(),
        Some(uuid(PROJECT_ID))
    );
    assert_eq!(
        saved.profiles["staging"].selected_project(),
        Some(uuid(STAGING_PROJECT))
    );
}

#[test]
fn exit_stops_the_runtime() {
    let (results, _incoming) = mpsc::unbounded_channel();
    let profile =
        crate::config::ActiveProfile::in_memory("default", crate::config::Profile::default(), None);
    let mut runtime = Runtime {
        worker: Worker::new(profile, results.clone()),
        results,
        config: ConfigFile::at(std::env::temp_dir().join("unused-config.toml"), None),
    };

    assert!(!runtime.execute(vec![Effect::Exit]));
}
