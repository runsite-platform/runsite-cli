use super::ApiClient;
use crate::config::{ActiveProfile, Profile};
use uuid::Uuid;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

pub const TEST_KEY: &str = "ak_live_test";
pub const PROJECT_ID: &str = "ca83c4a9-1517-4fe3-93aa-bbdab4eaee23";
pub const SERVICE_ID: &str = "3f2a1b4c-0000-4000-8000-000000000001";
pub const DATABASE_ID: &str = "3f2a1b4c-0000-4000-8000-000000000004";

pub fn uuid(text: &str) -> Uuid {
    Uuid::parse_str(text).unwrap()
}

pub fn client_for(server: &MockServer) -> ApiClient {
    client_in_project(server, None)
}

/// A client whose profile has `project_id` selected with `runsite project use`.
pub fn client_in_project(server: &MockServer, project_id: Option<&str>) -> ApiClient {
    session_in_project(server, project_id).0
}

/// A client and the in-memory profile it reads, for commands that take both.
pub fn session_in_project(
    server: &MockServer,
    project_id: Option<&str>,
) -> (ApiClient, ActiveProfile) {
    let profile = Profile {
        api_url: server.uri(),
        api_key: Some(TEST_KEY.to_string()),
        current_project_id: project_id.map(str::to_string),
    };
    let active = ActiveProfile::in_memory("default", profile, None);
    (ApiClient::new(active.clone()), active)
}

pub async fn serve(server: &MockServer, route: &str, body: serde_json::Value) {
    Mock::given(method("GET"))
        .and(path(route))
        .and(header(
            "authorization",
            format!("Bearer {TEST_KEY}").as_str(),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(server)
        .await;
}

/// Expect exactly one authenticated request and answer it with `body`.
pub async fn expect_request(
    server: &MockServer,
    http_method: &str,
    route: &str,
    status: u16,
    body: serde_json::Value,
) {
    Mock::given(method(http_method))
        .and(path(route))
        .and(header(
            "authorization",
            format!("Bearer {TEST_KEY}").as_str(),
        ))
        .respond_with(ResponseTemplate::new(status).set_body_json(body))
        .expect(1)
        .mount(server)
        .await;
}
