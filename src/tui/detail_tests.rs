use super::action::{Action, Effect, FetchError, Payload, Request};
use super::app::{update, Screen};
use super::detail::DetailTab;
use super::fixtures::*;
use super::log_buffer::LogQuery;
use ratatui::crossterm::event::KeyCode;

fn detail(app: &super::app::App) -> &super::detail::ServiceDetailState {
    match &app.screen {
        Screen::ServiceDetail(detail) => detail,
        other => panic!("expected service detail, got {other:?}"),
    }
}

fn log_fetches(effects: &[Effect]) -> Vec<LogQuery> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::FetchLogs { query, .. } => Some(*query),
            _ => None,
        })
        .collect()
}

#[test]
fn enter_on_a_service_opens_its_overview_and_polls_it() {
    let mut app = loaded_dashboard();
    update(&mut app, key(KeyCode::Tab));
    let effects = update(&mut app, key(KeyCode::Enter));
    assert_eq!(detail(&app).tab, DetailTab::Overview);
    // The latest deployment is already in flight from the dashboard.
    assert_eq!(
        fetched(&effects),
        vec![
            Request::Service(API_SERVICE),
            Request::Metrics(API_SERVICE),
            Request::MetricsHistory(API_SERVICE),
            Request::Deployments(API_SERVICE),
        ]
    );
    assert!(!app.poller.is_active(&Request::Projects));
}

#[test]
fn digits_on_the_dashboard_open_a_service_on_that_tab() {
    let mut app = service_detail('2');
    assert_eq!(detail(&app).tab, DetailTab::Logs);
    update(&mut app, key(KeyCode::Esc));
    assert!(matches!(app.screen, Screen::Dashboard));
}

#[test]
fn digits_are_ignored_on_databases() {
    let mut app = loaded_dashboard();
    update(&mut app, key(KeyCode::Tab));
    for _ in 0..3 {
        update(&mut app, char_key('j'));
    }
    update(&mut app, char_key('2'));
    assert!(matches!(app.screen, Screen::Dashboard));
}

#[test]
fn enter_on_postgres_opens_it_with_the_version_from_the_project() {
    let mut app = loaded_dashboard();
    update(&mut app, key(KeyCode::Tab));
    for _ in 0..3 {
        update(&mut app, char_key('j'));
    }
    let effects = update(&mut app, key(KeyCode::Enter));
    match &app.screen {
        Screen::DatabaseDetail(database) => {
            assert_eq!(database.version.as_deref(), Some("16"));
        }
        other => panic!("expected database detail, got {other:?}"),
    }
    assert!(fetched(&effects).contains(&Request::Database(POSTGRES)));
    deliver(
        &mut app,
        Request::Database(POSTGRES),
        Payload::Database(Box::new(postgres_detail())),
    );
    update(&mut app, key(KeyCode::Esc));
    assert!(matches!(app.screen, Screen::Dashboard));
}

#[test]
fn a_database_being_deleted_cannot_be_opened() {
    let mut app = loaded_dashboard();
    let mut detail = landing_detail();
    detail.databases_list[0].status = "pending_deletion".to_string();
    app.dashboard.details.insert(LANDING, detail);
    update(&mut app, key(KeyCode::Tab));
    for _ in 0..3 {
        update(&mut app, char_key('j'));
    }
    update(&mut app, key(KeyCode::Enter));
    assert!(matches!(app.screen, Screen::Dashboard));
    assert_eq!(app.toast.as_ref().unwrap().text, "Being deleted");
}

#[test]
fn a_deployment_in_progress_speeds_up_polling() {
    let mut app = service_detail('1');
    deliver(
        &mut app,
        Request::LatestDeployment(API_SERVICE),
        Payload::LatestDeployment(Some(Box::new(deployment(
            DEPLOYMENT_NEW,
            "building",
            false,
            "4b1c2d3e",
        )))),
    );
    assert!(detail(&app).transitional());
    deliver(
        &mut app,
        Request::Service(API_SERVICE),
        Payload::Service(Box::new(service(
            API_SERVICE,
            "api",
            "running",
            Some(LANDING),
        ))),
    );
    let effects = update(&mut app, Action::Tick { now: at(4) });
    assert!(fetched(&effects).contains(&Request::Service(API_SERVICE)));
}

#[test]
fn the_logs_tab_starts_with_a_tail_then_follows_the_cursor() {
    let mut app = service_detail('2');
    let effects = update(&mut app, Action::Tick { now: at(3) });
    assert!(
        log_fetches(&effects).is_empty(),
        "the first fetch already left"
    );

    let mut app = loaded_dashboard();
    update(&mut app, key(KeyCode::Tab));
    let effects = update(&mut app, char_key('2'));
    assert_eq!(
        log_fetches(&effects),
        vec![LogQuery {
            tail: 500,
            since: None
        }]
    );

    deliver(
        &mut app,
        Request::Logs(API_SERVICE),
        Payload::Logs {
            body: "2026-09-23T10:00:07Z started\n".to_string(),
            query: LogQuery {
                tail: 500,
                since: None,
            },
        },
    );
    let effects = update(&mut app, Action::Tick { now: at(6) });
    assert_eq!(
        log_fetches(&effects),
        vec![LogQuery {
            tail: 1000,
            since: Some(1_790_157_602)
        }]
    );
}

#[test]
fn a_400_from_logs_becomes_the_tab_message() {
    let mut app = service_detail('2');
    fail(
        &mut app,
        Request::Logs(API_SERVICE),
        FetchError::Rejected {
            status: 400,
            message: "Unable to fetch logs: container is starting".to_string(),
        },
    );
    assert_eq!(
        detail(&app).logs.status_message.as_deref(),
        Some("Unable to fetch logs: container is starting")
    );
    assert!(app.toast.is_none());
}

#[test]
fn the_filter_keys_on_the_logs_tab_are_text_while_searching() {
    let mut app = service_detail('2');
    update(&mut app, char_key('/'));
    for character in "q2".chars() {
        update(&mut app, char_key(character));
    }
    assert_eq!(detail(&app).logs.search, "q2");
    assert_eq!(detail(&app).tab, DetailTab::Logs);
    assert!(app.exit.is_none());
    update(&mut app, key(KeyCode::Enter));
    assert!(!detail(&app).logs.editing_search);
}

#[test]
fn arrows_scroll_logs_sideways_only_with_wrap_off() {
    let mut app = service_detail('2');
    update(&mut app, key(KeyCode::Right));
    assert_eq!(detail(&app).tab, DetailTab::Deploys);

    let mut app = service_detail('2');
    update(&mut app, char_key('w'));
    update(&mut app, key(KeyCode::Right));
    assert_eq!(detail(&app).tab, DetailTab::Logs);
    assert_eq!(detail(&app).logs.horizontal_offset, 8);
    update(&mut app, key(KeyCode::Tab));
    assert_eq!(detail(&app).tab, DetailTab::Deploys);
}

#[test]
fn enter_on_a_deployment_opens_its_detail_and_esc_returns() {
    let mut app = service_detail('3');
    deliver(
        &mut app,
        Request::Deployments(API_SERVICE),
        Payload::Deployments(deployments()),
    );
    update(&mut app, char_key('j'));
    let effects = update(&mut app, key(KeyCode::Enter));
    let view = detail(&app).deployment_view.as_ref().unwrap();
    assert_eq!(view.id, DEPLOYMENT_OLD);
    assert!(fetched(&effects).contains(&Request::Deployment(API_SERVICE, DEPLOYMENT_OLD)));

    update(&mut app, char_key('G'));
    assert_eq!(detail(&app).deployment_view.as_ref().unwrap().scroll, 2);
    update(&mut app, key(KeyCode::Esc));
    assert!(detail(&app).deployment_view.is_none());
    assert!(matches!(app.screen, Screen::ServiceDetail(_)));
}

#[test]
fn a_service_that_disappears_returns_to_the_dashboard() {
    let mut app = service_detail('1');
    fail(
        &mut app,
        Request::Service(API_SERVICE),
        FetchError::NotFound {
            message: "Web service not found".to_string(),
        },
    );
    assert!(matches!(app.screen, Screen::Dashboard));
    assert_eq!(app.toast.as_ref().unwrap().text, "No longer exists");
}

#[test]
fn returning_to_logs_after_thirty_seconds_starts_a_fresh_tail() {
    let mut app = service_detail('2');
    deliver(
        &mut app,
        Request::Logs(API_SERVICE),
        Payload::Logs {
            body: "2026-09-23T10:00:07Z started\n".to_string(),
            query: LogQuery {
                tail: 500,
                since: None,
            },
        },
    );
    update(&mut app, char_key('1'));
    update(&mut app, Action::Tick { now: at(40) });
    let effects = update(&mut app, char_key('2'));
    assert_eq!(
        log_fetches(&effects),
        vec![LogQuery {
            tail: 500,
            since: None
        }]
    );
}

#[test]
fn metrics_and_history_are_kept_for_the_overview() {
    let mut app = service_detail('1');
    deliver(
        &mut app,
        Request::Metrics(API_SERVICE),
        Payload::Metrics(metrics()),
    );
    deliver(
        &mut app,
        Request::MetricsHistory(API_SERVICE),
        Payload::MetricsHistory(history()),
    );
    assert_eq!(detail(&app).metrics, Some(metrics()));
    assert_eq!(detail(&app).history.len(), 12);
}

#[test]
fn a_log_reply_from_before_reopening_is_dropped() {
    let mut app = service_detail('2');
    let stale_query = LogQuery {
        tail: 1000,
        since: Some(1_790_157_595),
    };
    deliver(
        &mut app,
        Request::Logs(API_SERVICE),
        Payload::Logs {
            body: "2026-09-23T10:00:20Z newer\n".to_string(),
            query: stale_query,
        },
    );
    assert!(detail(&app).logs.lines().is_empty());
}

#[test]
fn a_service_being_deleted_cannot_be_opened() {
    let mut app = loaded_dashboard();
    let mut project = landing_detail();
    project.web_services_list[0].status = "pending_deletion".to_string();
    app.dashboard.details.insert(LANDING, project);
    update(&mut app, key(KeyCode::Tab));
    update(&mut app, key(KeyCode::Enter));
    assert!(matches!(app.screen, Screen::Dashboard));
    assert_eq!(app.toast.as_ref().unwrap().text, "Being deleted");
}
