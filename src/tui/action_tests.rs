use super::action::{Action, Effect, FetchError, Mutation, Payload, Request};
use super::app::{update, App, Overlay, Screen, RATE_LIMIT_MESSAGE};
use super::fixtures::*;
use super::status::Tone;
use ratatui::crossterm::event::KeyCode;

fn mutations(effects: &[Effect]) -> Vec<Mutation> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Mutate { mutation, .. } => Some(*mutation),
            _ => None,
        })
        .collect()
}

fn dashboard_on_resources() -> App {
    let mut app = loaded_dashboard();
    update(&mut app, key(KeyCode::Tab));
    no_deployment_running(&mut app, API_SERVICE);
    app
}

fn confirm_prompt(app: &App) -> String {
    match &app.overlay {
        Some(Overlay::Confirm(confirm)) => {
            format!("{} {}{}", confirm.verb, confirm.target, confirm.suffix)
        }
        other => panic!("expected a confirm modal, got {other:?}"),
    }
}

/// Service detail on Overview with every in-flight request answered, so that
/// a refresh can schedule them again.
fn settled_service_detail() -> App {
    let mut app = service_detail('1');
    let api = service(API_SERVICE, "api", "running", Some(LANDING));
    deliver(
        &mut app,
        Request::Service(API_SERVICE),
        Payload::Service(Box::new(api)),
    );
    deliver(
        &mut app,
        Request::LatestDeployment(API_SERVICE),
        Payload::LatestDeployment(None),
    );
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
    deliver(
        &mut app,
        Request::Deployments(API_SERVICE),
        Payload::Deployments(deployments()),
    );
    app
}

fn mutated(app: &mut App, mutation: Mutation, result: Result<(), FetchError>) -> Vec<Effect> {
    let action = Action::Mutated {
        generation: app.generation,
        mutation,
        result,
    };
    update(app, action)
}

#[test]
fn a_freshly_opened_service_waits_for_the_deployment_check() {
    let mut app = service_detail('1');
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
    update(&mut app, char_key('D'));
    assert!(app.overlay.is_none());
    assert!(app
        .toast
        .as_ref()
        .is_some_and(|toast| toast.text.contains("checking for a deployment in progress")));

    deliver(
        &mut app,
        Request::LatestDeployment(API_SERVICE),
        Payload::LatestDeployment(None),
    );
    update(&mut app, char_key('D'));
    assert!(matches!(app.overlay, Some(Overlay::Confirm(_))));
}

#[test]
fn a_service_selected_again_waits_for_a_fresh_deployment_check() {
    let mut app = dashboard_on_resources();
    update(&mut app, char_key('j'));
    update(&mut app, char_key('k'));
    update(&mut app, char_key('D'));
    assert!(app.overlay.is_none());
    assert!(app
        .toast
        .as_ref()
        .is_some_and(|toast| toast.text.contains("checking for a deployment in progress")));
}

#[test]
fn restart_asks_first_and_y_sends_exactly_one_request() {
    let mut app = dashboard_on_resources();
    let effects = update(&mut app, char_key('R'));
    assert!(mutations(&effects).is_empty());
    assert_eq!(confirm_prompt(&app), "Restart api?");

    let effects = update(&mut app, char_key('y'));
    assert_eq!(
        mutations(&effects),
        vec![Mutation::Restart {
            service_id: API_SERVICE
        }]
    );
    assert!(app.overlay.is_none());
}

#[test]
fn n_esc_and_enter_cancel_without_a_request() {
    for cancel in [char_key('n'), key(KeyCode::Esc), key(KeyCode::Enter)] {
        let mut app = dashboard_on_resources();
        update(&mut app, char_key('D'));
        let effects = update(&mut app, cancel);
        assert!(mutations(&effects).is_empty());
        assert!(app.overlay.is_none());
    }
}

#[test]
fn other_keys_leave_the_modal_open() {
    let mut app = dashboard_on_resources();
    update(&mut app, char_key('D'));
    for character in ['Y', 'q', 'D', 'j'] {
        assert!(mutations(&update(&mut app, char_key(character))).is_empty());
    }
    assert!(matches!(app.overlay, Some(Overlay::Confirm(_))));
    assert!(app.exit.is_none());
}

#[test]
fn deploy_names_the_branch_or_the_image() {
    let mut app = dashboard_on_resources();
    update(&mut app, char_key('D'));
    assert_eq!(confirm_prompt(&app), "Deploy api from branch main?");

    let mut app = dashboard_on_resources();
    let mut detail = landing_detail();
    detail.web_services_list[0].source_type = Some("image".to_string());
    detail.web_services_list[0].image_ref = Some("nginx:1.27".to_string());
    app.dashboard.details.insert(LANDING, detail);
    update(&mut app, char_key('D'));
    assert_eq!(confirm_prompt(&app), "Deploy api from image nginx:1.27?");
}

#[test]
fn a_disabled_action_explains_itself_without_a_modal() {
    let mut app = dashboard_on_resources();
    update(&mut app, char_key('j'));
    update(&mut app, char_key('j'));
    no_deployment_running(&mut app, WORKER_SERVICE);
    update(&mut app, char_key('R'));
    assert!(app.overlay.is_none());
    assert_eq!(
        app.toast.as_ref().unwrap().text,
        "Restart is unavailable: the service is stopped"
    );
}

#[test]
fn a_read_key_disables_actions() {
    let mut app = dashboard_on_resources();
    deliver(
        &mut app,
        Request::KeyScope,
        Payload::KeyScope(crate::api::ApiKeyIdentity {
            id: uuid::Uuid::from_u128(9),
            name: "ci".to_string(),
            scope: "read".to_string(),
        }),
    );
    update(&mut app, char_key('D'));
    assert!(app.overlay.is_none());
    assert_eq!(
        app.toast.as_ref().unwrap().text,
        "Deploy is unavailable: needs a write key"
    );
}

#[test]
fn dashboard_actions_need_the_resources_pane() {
    let mut app = loaded_dashboard();
    assert!(update(&mut app, char_key('R')).is_empty());
    assert!(app.overlay.is_none());
    assert!(app.toast.is_none());
}

#[test]
fn dashboard_deploy_waits_for_the_deployment_check() {
    let mut app = loaded_dashboard();
    update(&mut app, key(KeyCode::Tab));
    update(&mut app, char_key('D'));
    assert!(app.overlay.is_none());
    assert_eq!(
        app.toast.as_ref().unwrap().text,
        "Deploy is unavailable: checking for a deployment in progress"
    );

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
    update(&mut app, char_key('D'));
    assert_eq!(
        app.toast.as_ref().unwrap().text,
        "Deploy is unavailable: a deployment is in progress"
    );
}

#[test]
fn s_toggles_a_database_on_the_dashboard() {
    let mut app = dashboard_on_resources();
    for _ in 0..3 {
        update(&mut app, char_key('j'));
    }
    update(&mut app, char_key('S'));
    assert_eq!(confirm_prompt(&app), "Stop pg-main?");
    let effects = update(&mut app, char_key('y'));
    assert_eq!(
        mutations(&effects),
        vec![Mutation::StopDatabase {
            database_id: POSTGRES
        }]
    );
}

#[test]
fn deploy_is_disabled_while_a_deployment_is_in_progress() {
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
    update(&mut app, char_key('D'));
    assert!(app.overlay.is_none());
    assert_eq!(
        app.toast.as_ref().unwrap().text,
        "Deploy is unavailable: a deployment is in progress"
    );
}

#[test]
fn b_rolls_back_to_the_selected_ready_deployment() {
    let mut app = service_detail('3');
    deliver(
        &mut app,
        Request::Deployments(API_SERVICE),
        Payload::Deployments(deployments()),
    );
    update(&mut app, char_key('B'));
    assert!(
        app.overlay.is_none(),
        "the live deployment cannot be restored"
    );

    update(&mut app, char_key('j'));
    update(&mut app, char_key('B'));
    assert_eq!(confirm_prompt(&app), "Roll back api to 7b3e0f2?");
    let effects = update(&mut app, char_key('y'));
    assert_eq!(
        mutations(&effects),
        vec![Mutation::Rollback {
            service_id: API_SERVICE,
            deployment_id: DEPLOYMENT_OLD
        }]
    );
}

#[test]
fn b_is_ignored_outside_deploys() {
    let mut app = service_detail('1');
    assert!(update(&mut app, char_key('B')).is_empty());
    assert!(app.overlay.is_none());
    assert!(app.toast.is_none());
}

#[test]
fn an_accepted_action_refreshes_the_screen() {
    let mut app = settled_service_detail();
    let effects = mutated(
        &mut app,
        Mutation::Restart {
            service_id: API_SERVICE,
        },
        Ok(()),
    );
    assert_eq!(app.toast.as_ref().unwrap().text, "Restart requested");
    assert_eq!(app.toast.as_ref().unwrap().tone, Tone::Good);
    assert!(fetched(&effects).contains(&Request::Metrics(API_SERVICE)));
}

#[test]
fn a_rate_limited_action_is_not_retried() {
    let mut app = service_detail('1');
    let effects = mutated(
        &mut app,
        Mutation::Deploy {
            service_id: API_SERVICE,
        },
        Err(FetchError::RateLimited),
    );
    assert!(mutations(&effects).is_empty());
    assert_eq!(app.toast.as_ref().unwrap().text, RATE_LIMIT_MESSAGE);
    assert!(
        !app.poller.backing_off(),
        "polling is not slowed by a mutation"
    );
}

#[test]
fn a_failed_action_shows_the_api_detail_and_refetches() {
    let mut app = settled_service_detail();
    let effects = mutated(
        &mut app,
        Mutation::StopService {
            service_id: API_SERVICE,
        },
        Err(FetchError::Forbidden {
            message: "This action needs a `write` key (yours: `read`)".to_string(),
        }),
    );
    assert_eq!(
        app.toast.as_ref().unwrap().text,
        "This action needs a `write` key (yours: `read`)"
    );
    assert!(fetched(&effects).contains(&Request::Service(API_SERVICE)));
    assert!(matches!(app.screen, Screen::ServiceDetail(_)));
}

#[test]
fn an_action_on_a_deleted_service_returns_to_the_dashboard() {
    let mut app = service_detail('1');
    mutated(
        &mut app,
        Mutation::Restart {
            service_id: API_SERVICE,
        },
        Err(FetchError::NotFound {
            message: "Web service not found".to_string(),
        }),
    );
    assert!(matches!(app.screen, Screen::Dashboard));
}

#[test]
fn a_result_from_before_a_profile_switch_is_dropped() {
    let mut app = service_detail('1');
    app.generation = 3;
    update(
        &mut app,
        Action::Mutated {
            generation: 2,
            mutation: Mutation::Restart {
                service_id: API_SERVICE,
            },
            result: Ok(()),
        },
    );
    assert!(app.toast.is_none());
}
