use super::action::{Action, Effect, FetchError, Payload, ProfileEntry, Request};
use super::app::{update, ExitReason, Overlay, Screen, INVALID_KEY_NOTICE, RATE_LIMIT_MESSAGE};
use super::dashboard::{Pane, ProjectChoice};
use super::fixtures::*;
use ratatui::crossterm::event::KeyCode;

fn offline() -> FetchError {
    FetchError::Offline {
        message: "connection refused".to_string(),
    }
}

#[test]
fn a_logged_in_start_fetches_identity_and_projects() {
    let mut app = logged_in_app();
    let effects = app.start();
    assert_eq!(
        fetched(&effects),
        vec![
            Request::CurrentUser,
            Request::KeyScope,
            Request::Projects,
            Request::UnassignedServices
        ]
    );
}

#[test]
fn a_start_without_credentials_opens_login_and_fetches_nothing() {
    let mut app = super::app::App::new(session(false), None, at(0), (120, 40));
    assert!(app.start().is_empty());
    assert!(matches!(app.screen, Screen::Login(_)));
}

#[test]
fn loaded_projects_select_the_first_and_fetch_its_resources() {
    let mut app = logged_in_app();
    app.start();
    let effects = deliver(&mut app, Request::Projects, Payload::Projects(projects()));
    assert_eq!(
        app.dashboard.selected_project,
        Some(ProjectChoice::Project(LANDING))
    );
    assert_eq!(fetched(&effects), vec![Request::ProjectDetail(LANDING)]);
}

#[test]
fn the_project_saved_in_the_profile_is_preselected() {
    let mut app = super::app::App::new(session(true), Some(BLOG), at(0), (120, 40));
    app.start();
    let effects = deliver(&mut app, Request::Projects, Payload::Projects(projects()));
    assert_eq!(
        app.dashboard.selected_project,
        Some(ProjectChoice::Project(BLOG))
    );
    assert_eq!(fetched(&effects), vec![Request::ProjectDetail(BLOG)]);
}

#[test]
fn moving_the_cursor_remembers_the_project_without_writing_anything() {
    let mut app = loaded_dashboard();
    let effects = update(&mut app, char_key('j'));
    assert_eq!(
        app.dashboard.selected_project,
        Some(ProjectChoice::Project(BLOG))
    );
    assert_eq!(app.chosen_projects.get("default"), Some(&BLOG));
    assert_eq!(fetched(&effects), vec![Request::ProjectDetail(BLOG)]);
    assert!(effects
        .iter()
        .all(|effect| matches!(effect, Effect::Fetch { .. })));
}

#[test]
fn the_unassigned_row_lists_only_services_without_a_project() {
    let mut app = loaded_dashboard();
    update(&mut app, char_key('j'));
    update(&mut app, char_key('j'));
    assert_eq!(
        app.dashboard.selected_project,
        Some(ProjectChoice::Unassigned)
    );
    let names: Vec<String> = app
        .dashboard
        .resources()
        .iter()
        .map(|resource| resource.name().to_string())
        .collect();
    assert_eq!(names, vec!["scratch"]);
}

#[test]
fn resources_are_ordered_services_then_postgres_then_redis() {
    let app = loaded_dashboard();
    let names: Vec<String> = app
        .dashboard
        .resources()
        .iter()
        .map(|resource| resource.name().to_string())
        .collect();
    assert_eq!(names, vec!["api", "web", "worker", "pg-main", "cache"]);
}

#[test]
fn a_result_from_an_older_generation_is_dropped() {
    let mut app = logged_in_app();
    app.start();
    app.generation = 1;
    update(
        &mut app,
        Action::Loaded {
            generation: 0,
            request: Request::Projects,
            result: Ok(Payload::Projects(projects())),
        },
    );
    assert!(app.dashboard.projects.is_none());
}

#[test]
fn a_result_for_a_request_that_is_no_longer_polled_is_dropped() {
    let mut app = loaded_dashboard();
    update(&mut app, char_key('j'));
    let mut stale = landing_detail();
    stale.name = "renamed".to_string();
    app.dashboard.details.clear();
    deliver(
        &mut app,
        Request::ProjectDetail(LANDING),
        Payload::ProjectDetail(stale),
    );
    assert!(!app.dashboard.details.contains_key(&LANDING));
}

#[test]
fn an_invalid_profile_key_opens_login_with_an_explanation() {
    let mut app = loaded_dashboard();
    fail(
        &mut app,
        Request::Projects,
        FetchError::Unauthorized {
            message: "Invalid API key".to_string(),
        },
    );
    match &app.screen {
        Screen::Login(login) => assert_eq!(login.notice.as_deref(), Some(INVALID_KEY_NOTICE)),
        other => panic!("expected login, got {other:?}"),
    }
    assert!(!app.poller.is_active(&Request::Projects));
}

#[test]
fn an_invalid_env_token_exits_instead_of_opening_login() {
    let mut app = loaded_dashboard();
    app.session.env_token = true;
    let effects = fail(
        &mut app,
        Request::Projects,
        FetchError::Unauthorized {
            message: "Invalid API key".to_string(),
        },
    );
    assert!(matches!(effects.as_slice(), [Effect::Exit]));
    assert_eq!(app.exit, Some(ExitReason::EnvTokenRejected));
}

#[test]
fn a_blocked_account_shows_the_reason_and_only_q_works() {
    let mut app = loaded_dashboard();
    fail(
        &mut app,
        Request::Projects,
        FetchError::Blocked {
            reason: "unpaid invoice".to_string(),
        },
    );
    assert!(matches!(&app.screen, Screen::Blocked { reason } if reason == "unpaid invoice"));

    assert!(update(&mut app, char_key('?')).is_empty());
    assert!(app.overlay.is_none());
    assert!(matches!(
        update(&mut app, char_key('q')).as_slice(),
        [Effect::Exit]
    ));
}

#[test]
fn a_network_error_marks_the_app_offline_until_the_next_success() {
    let mut app = loaded_dashboard();
    app.poller.refresh_all(app.now);
    let effects = update(&mut app, Action::Tick { now: at(4) });
    assert!(fetched(&effects).contains(&Request::Projects));

    fail(&mut app, Request::Projects, offline());
    assert!(app.poller.backing_off());
    assert!(
        app.dashboard.projects.is_some(),
        "stale data stays on screen"
    );

    let retry = update(&mut app, Action::Tick { now: at(6) });
    assert!(fetched(&retry).contains(&Request::Projects));
    deliver(&mut app, Request::Projects, Payload::Projects(projects()));
    assert!(!app.poller.backing_off());
}

#[test]
fn a_rate_limit_backs_off_and_explains_itself() {
    let mut app = loaded_dashboard();
    app.poller.refresh_all(app.now);
    update(&mut app, Action::Tick { now: at(4) });
    fail(&mut app, Request::Projects, FetchError::RateLimited);
    assert!(app.poller.backing_off());
    assert_eq!(app.toast.as_ref().unwrap().text, RATE_LIMIT_MESSAGE);
}

#[test]
fn a_deleted_project_is_dropped_with_a_toast() {
    let mut app = loaded_dashboard();
    app.poller.refresh_all(app.now);
    update(&mut app, Action::Tick { now: at(4) });
    fail(
        &mut app,
        Request::ProjectDetail(LANDING),
        FetchError::NotFound {
            message: "Project not found".to_string(),
        },
    );
    assert!(!app.dashboard.details.contains_key(&LANDING));
    assert_eq!(app.toast.as_ref().unwrap().text, "No longer exists");
}

#[test]
fn toasts_expire() {
    let mut app = loaded_dashboard();
    app.show_toast("hello", super::status::Tone::Bad);
    update(&mut app, Action::Tick { now: at(6) });
    assert!(app.toast.is_some());
    update(&mut app, Action::Tick { now: at(8) });
    assert!(app.toast.is_none());
}

#[test]
fn the_filter_captures_every_printable_key() {
    let mut app = loaded_dashboard();
    update(&mut app, char_key('/'));
    for character in "qb".chars() {
        assert!(update(&mut app, char_key(character)).is_empty());
    }
    assert_eq!(app.dashboard.project_filter, "qb");
    assert!(app.exit.is_none());

    update(&mut app, key(KeyCode::Backspace));
    update(&mut app, key(KeyCode::Backspace));
    update(&mut app, char_key('b'));
    assert_eq!(
        app.dashboard.selected_project,
        Some(ProjectChoice::Project(BLOG))
    );

    update(&mut app, key(KeyCode::Esc));
    assert_eq!(app.dashboard.project_filter, "");
    assert_eq!(app.dashboard.editing_filter, Some(Pane::Projects));
    update(&mut app, key(KeyCode::Esc));
    assert_eq!(app.dashboard.editing_filter, None);
}

#[test]
fn ctrl_c_quits_even_while_typing() {
    let mut app = loaded_dashboard();
    update(&mut app, char_key('/'));
    let effects = update(&mut app, ctrl(KeyCode::Char('c')));
    assert!(matches!(effects.as_slice(), [Effect::Exit]));
}

#[test]
fn ctrl_z_is_ignored() {
    let mut app = loaded_dashboard();
    assert!(update(&mut app, ctrl(KeyCode::Char('z'))).is_empty());
    assert!(app.exit.is_none());
}

#[test]
fn tab_switches_the_focused_pane() {
    let mut app = loaded_dashboard();
    update(&mut app, key(KeyCode::Tab));
    assert_eq!(app.dashboard.focus, Pane::Resources);
    update(&mut app, char_key('j'));
    assert_eq!(app.dashboard.resource_cursor, 1);
    update(&mut app, char_key('h'));
    assert_eq!(app.dashboard.focus, Pane::Projects);
}

#[test]
fn refresh_fetches_every_periodic_request_now() {
    let mut app = loaded_dashboard();
    let effects = update(&mut app, char_key('r'));
    assert_eq!(
        fetched(&effects),
        vec![
            Request::Projects,
            Request::ProjectDetail(LANDING),
            Request::UnassignedServices
        ]
    );
}

#[test]
fn submitting_the_login_form_sends_the_credentials_once() {
    let mut app = super::app::App::new(session(false), None, at(0), (120, 40));
    for character in "ada@example.com".chars() {
        update(&mut app, char_key(character));
    }
    update(&mut app, key(KeyCode::Tab));
    for character in "hunter2".chars() {
        update(&mut app, char_key(character));
    }
    let effects = update(&mut app, key(KeyCode::Enter));
    assert!(matches!(
        effects.as_slice(),
        [Effect::LogIn { generation: 0, .. }]
    ));
    assert!(update(&mut app, key(KeyCode::Enter)).is_empty());
}

#[test]
fn an_empty_login_form_is_rejected_inline() {
    let mut app = super::app::App::new(session(false), None, at(0), (120, 40));
    assert!(update(&mut app, key(KeyCode::Enter)).is_empty());
    match &app.screen {
        Screen::Login(login) => assert!(login.error.is_some()),
        other => panic!("expected login, got {other:?}"),
    }
}

#[test]
fn a_successful_login_opens_the_dashboard_and_starts_fetching() {
    let mut app = super::app::App::new(session(false), None, at(0), (120, 40));
    let effects = update(
        &mut app,
        Action::LoggedIn {
            generation: 0,
            result: Ok("ada@example.com".to_string()),
        },
    );
    assert!(matches!(app.screen, Screen::Dashboard));
    assert!(fetched(&effects).contains(&Request::Projects));
}

#[test]
fn a_failed_login_shows_the_server_message() {
    let mut app = super::app::App::new(session(false), None, at(0), (120, 40));
    update(
        &mut app,
        Action::LoggedIn {
            generation: 0,
            result: Err(FetchError::Forbidden {
                message: "Your account is awaiting review".to_string(),
            }),
        },
    );
    match &app.screen {
        Screen::Login(login) => {
            assert_eq!(
                login.error.as_deref(),
                Some("Your account is awaiting review")
            );
            assert!(!login.submitting);
        }
        other => panic!("expected login, got {other:?}"),
    }
}

fn profile(name: &str, has_key: bool) -> ProfileEntry {
    ProfileEntry {
        name: name.to_string(),
        api_url: format!("https://{name}.runsite.app"),
        has_key,
        current_project_id: None,
    }
}

#[test]
fn switching_profiles_bumps_the_generation_and_reloads() {
    let mut app = loaded_dashboard();
    let effects = update(&mut app, char_key('P'));
    assert!(matches!(effects.as_slice(), [Effect::ListProfiles]));
    update(
        &mut app,
        Action::ProfilesListed(vec![profile("default", true), profile("staging", true)]),
    );
    update(&mut app, char_key('j'));
    let effects = update(&mut app, key(KeyCode::Enter));

    assert_eq!(app.generation, 1);
    assert_eq!(app.session.profile, "staging");
    assert!(app.dashboard.projects.is_none());
    assert!(matches!(
        effects.first(),
        Some(Effect::SwitchProfile { profile, .. }) if profile == "staging"
    ));
    assert!(fetched(&effects).contains(&Request::Projects));
    assert!(effects.iter().all(|effect| match effect {
        Effect::Fetch { generation, .. } => *generation == 1,
        _ => true,
    }));
}

#[test]
fn switching_to_a_profile_without_a_key_opens_login() {
    let mut app = loaded_dashboard();
    update(&mut app, char_key('P'));
    update(
        &mut app,
        Action::ProfilesListed(vec![profile("default", true), profile("fresh", false)]),
    );
    update(&mut app, char_key('j'));
    let effects = update(&mut app, key(KeyCode::Enter));
    assert!(matches!(app.screen, Screen::Login(_)));
    assert!(fetched(&effects).is_empty());
}

#[test]
fn profiles_are_disabled_under_the_env_token() {
    let mut app = loaded_dashboard();
    app.session.env_token = true;
    assert!(update(&mut app, char_key('P')).is_empty());
    assert!(app.overlay.is_none());
    assert!(app.toast.is_some());
}

#[test]
fn any_key_closes_help() {
    let mut app = loaded_dashboard();
    update(&mut app, char_key('?'));
    assert!(matches!(app.overlay, Some(Overlay::Help)));
    update(&mut app, char_key('x'));
    assert!(app.overlay.is_none());
}

#[test]
fn a_late_401_for_the_old_key_does_not_undo_a_login() {
    let mut app = loaded_dashboard();
    app.poller.refresh_all(app.now);
    update(&mut app, Action::Tick { now: at(4) });
    let old_generation = app.generation;
    fail(
        &mut app,
        Request::UnassignedServices,
        FetchError::Unauthorized {
            message: "Invalid API key".to_string(),
        },
    );
    let generation = app.generation;
    update(
        &mut app,
        Action::LoggedIn {
            generation,
            result: Ok("ada@example.com".to_string()),
        },
    );
    update(
        &mut app,
        Action::Loaded {
            generation: old_generation,
            request: Request::Projects,
            result: Err(FetchError::Unauthorized {
                message: "Invalid API key".to_string(),
            }),
        },
    );
    assert!(matches!(app.screen, Screen::Dashboard));
}

#[test]
fn a_blocked_account_at_login_gets_the_blocked_screen() {
    let mut app = super::app::App::new(session(false), None, at(0), (120, 40));
    update(
        &mut app,
        Action::LoggedIn {
            generation: 0,
            result: Err(FetchError::Blocked {
                reason: String::new(),
            }),
        },
    );
    assert!(matches!(&app.screen, Screen::Blocked { .. }));
}

#[test]
fn esc_on_an_empty_login_field_leaves_the_form() {
    let mut app = super::app::App::new(session(false), None, at(0), (120, 40));
    update(&mut app, key(KeyCode::Esc));
    let effects = update(&mut app, char_key('P'));
    assert!(matches!(effects.as_slice(), [Effect::ListProfiles]));
    update(&mut app, key(KeyCode::Esc));

    update(&mut app, key(KeyCode::Enter));
    update(&mut app, char_key('q'));
    match &app.screen {
        Screen::Login(login) => assert_eq!(login.email, "q", "back in the form, q is text"),
        other => panic!("expected login, got {other:?}"),
    }
    update(&mut app, key(KeyCode::Esc));
    update(&mut app, key(KeyCode::Esc));
    assert!(matches!(
        update(&mut app, char_key('q')).as_slice(),
        [Effect::Exit]
    ));
}

#[test]
fn a_deleted_project_is_no_longer_polled() {
    let mut app = loaded_dashboard();
    app.poller.refresh_all(app.now);
    update(&mut app, Action::Tick { now: at(4) });
    fail(
        &mut app,
        Request::ProjectDetail(LANDING),
        FetchError::NotFound {
            message: "Project not found".to_string(),
        },
    );
    assert!(!app.poller.is_active(&Request::ProjectDetail(LANDING)));
    assert_eq!(
        app.dashboard.selected_project,
        Some(ProjectChoice::Project(BLOG))
    );
}

#[test]
fn returning_to_a_profile_keeps_the_project_chosen_in_this_session() {
    let mut app = loaded_dashboard();
    update(&mut app, char_key('j'));
    for target in ["staging", "default"] {
        update(&mut app, char_key('P'));
        update(
            &mut app,
            Action::ProfilesListed(vec![profile("default", true), profile("staging", true)]),
        );
        let steps = if target == "staging" { 1 } else { 0 };
        update(&mut app, key(KeyCode::Up));
        for _ in 0..steps {
            update(&mut app, char_key('j'));
        }
        update(&mut app, key(KeyCode::Enter));
    }
    assert_eq!(app.session.profile, "default");
    assert_eq!(
        app.dashboard.selected_project,
        Some(ProjectChoice::Project(BLOG))
    );
}

#[test]
fn a_failed_read_is_kept_as_the_latest_error_until_a_success() {
    let mut app = loaded_dashboard();
    app.poller.refresh_all(app.now);
    update(&mut app, Action::Tick { now: at(4) });
    fail(
        &mut app,
        Request::Projects,
        FetchError::Rejected {
            status: 400,
            message: "Bad request".to_string(),
        },
    );
    assert_eq!(app.last_error.as_deref(), Some("Bad request"));
    app.poller.refresh_all(app.now);
    update(&mut app, Action::Tick { now: at(40) });
    deliver(&mut app, Request::Projects, Payload::Projects(projects()));
    assert_eq!(app.last_error, None);
}
