//! Builders shared by the TUI tests.

use super::action::{Action, Effect, FetchError, Payload, Request};
use super::app::{App, Session};
use crate::api::{
    CurrentUser, PostgresInProject, ProjectDetail, ProjectSummary, RedisInProject, ServiceInfo,
    ServiceSummary,
};
use chrono::{DateTime, TimeZone, Utc};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use uuid::Uuid;

pub const LANDING: Uuid = Uuid::from_u128(0xca83c4a9_1517_4fe3_93aa_bbdab4eaee23);
pub const BLOG: Uuid = Uuid::from_u128(0x8d5b5e8e_2348_4a29_8c05_bcfa1e599ef2);
pub const API_SERVICE: Uuid = Uuid::from_u128(0x3f2a1b4c_0000_4000_8000_000000000001);
pub const WEB_SERVICE: Uuid = Uuid::from_u128(0x3f2a1b4c_0000_4000_8000_000000000002);
pub const WORKER_SERVICE: Uuid = Uuid::from_u128(0x3f2a1b4c_0000_4000_8000_000000000003);
pub const POSTGRES: Uuid = Uuid::from_u128(0x3f2a1b4c_0000_4000_8000_000000000004);
pub const REDIS: Uuid = Uuid::from_u128(0x3f2a1b4c_0000_4000_8000_000000000005);
pub const LOOSE_SERVICE: Uuid = Uuid::from_u128(0x3f2a1b4c_0000_4000_8000_000000000006);

pub fn at(second: i64) -> DateTime<Utc> {
    Utc.timestamp_opt(1_790_000_000 + second, 0).unwrap()
}

pub fn session(has_credentials: bool) -> Session {
    Session {
        profile: "default".to_string(),
        api_url: "https://api.runsite.app".to_string(),
        env_token: false,
        has_credentials,
        user: None,
        key_scope: None,
    }
}

pub fn logged_in_app() -> App {
    App::new(session(true), None, at(0), (120, 40))
}

pub fn key(code: KeyCode) -> Action {
    Action::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

pub fn char_key(character: char) -> Action {
    key(KeyCode::Char(character))
}

pub fn ctrl(code: KeyCode) -> Action {
    Action::Key(KeyEvent::new(code, KeyModifiers::CONTROL))
}

pub fn fetched(effects: &[Effect]) -> Vec<Request> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Fetch { request, .. } => Some(*request),
            _ => None,
        })
        .collect()
}

/// Deliver a successful response for `request` in the current generation.
pub fn deliver(app: &mut App, request: Request, payload: Payload) -> Vec<Effect> {
    let action = Action::Loaded {
        generation: app.generation,
        request,
        result: Ok(payload),
    };
    super::app::update(app, action)
}

/// Deliver a failed response for `request` in the current generation.
pub fn fail(app: &mut App, request: Request, error: FetchError) -> Vec<Effect> {
    let action = Action::Loaded {
        generation: app.generation,
        request,
        result: Err(error),
    };
    super::app::update(app, action)
}

pub fn user() -> CurrentUser {
    CurrentUser {
        id: Uuid::from_u128(1),
        email: "ada@example.com".to_string(),
    }
}

fn project(id: Uuid, name: &str, web_services: i64, postgresql: i64) -> ProjectSummary {
    ProjectSummary {
        id,
        name: name.to_string(),
        service_summary: ServiceSummary {
            web_services,
            postgresql,
            redis: 0,
        },
    }
}

pub fn projects() -> Vec<ProjectSummary> {
    vec![
        project(LANDING, "landing", 3, 1),
        project(BLOG, "blog", 1, 0),
    ]
}

pub fn service(id: Uuid, name: &str, status: &str, project_id: Option<Uuid>) -> ServiceInfo {
    ServiceInfo {
        id,
        name: name.to_string(),
        status: status.to_string(),
        url: Some(format!("https://{name}-x.runsite.app")),
        project_id,
        project_type: Some("web_service".to_string()),
        source_type: Some("git".to_string()),
        image_ref: None,
        github_branch: Some("main".to_string()),
        min_instances: Some(1),
        max_instances: Some(1),
    }
}

pub fn landing_detail() -> ProjectDetail {
    let mut api = service(API_SERVICE, "api", "running", Some(LANDING));
    api.max_instances = Some(3);
    ProjectDetail {
        id: LANDING,
        name: "landing".to_string(),
        web_services_list: vec![
            api,
            service(WEB_SERVICE, "web", "running", Some(LANDING)),
            service(WORKER_SERVICE, "worker", "stopped", Some(LANDING)),
        ],
        databases_list: vec![PostgresInProject {
            id: POSTGRES,
            name: "pg-main".to_string(),
            status: "running".to_string(),
            postgres_version: Some("16".to_string()),
        }],
        redis_list: vec![RedisInProject {
            id: REDIS,
            name: "cache".to_string(),
            status: "stopped".to_string(),
            redis_version: Some("7.2".to_string()),
        }],
    }
}

pub fn unassigned() -> Vec<ServiceInfo> {
    vec![
        service(LOOSE_SERVICE, "scratch", "sleeping", None),
        service(API_SERVICE, "api", "running", Some(LANDING)),
    ]
}

/// A logged-in app showing the loaded dashboard with `landing` selected.
pub fn loaded_dashboard() -> App {
    let mut app = logged_in_app();
    app.start();
    let steps = [
        (Request::CurrentUser, Payload::CurrentUser(user())),
        (Request::Projects, Payload::Projects(projects())),
        (
            Request::UnassignedServices,
            Payload::UnassignedServices(unassigned()),
        ),
    ];
    for (request, payload) in steps {
        deliver(&mut app, request, payload);
    }
    deliver(
        &mut app,
        Request::ProjectDetail(LANDING),
        Payload::ProjectDetail(landing_detail()),
    );
    app.now = at(3);
    app
}
