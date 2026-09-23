use super::tests::render_text;
use crate::tui::action::{Action, FetchError, Payload, ProfileEntry, Request};
use crate::tui::app::{update, App};
use crate::tui::fixtures::*;
use crate::tui::theme::Theme;
use insta::assert_snapshot;

fn unicode() -> Theme {
    Theme::from_env(|_| None)
}

fn ascii() -> Theme {
    Theme::from_env(|key| (key == "RUNSITE_ASCII").then(|| "1".to_string()))
}

#[test]
fn dashboard_loaded_wide() {
    let app = loaded_dashboard();
    let screen = render_text(&app, &unicode(), 120, 40);
    assert!(screen.contains("ada@example.com · profile: default"));
    assert!(screen.contains("▸ landing"));
    assert!(screen.contains("api"));
    assert!(screen.contains("1–3"));
    assert!(screen.contains("postgres 16"));
    assert!(screen.contains("Unassigned"));
    assert!(screen.contains("updated 3s ago"));
    assert_snapshot!(screen);
}

#[test]
fn dashboard_loaded_narrow_shows_one_pane() {
    let mut app = loaded_dashboard();
    let screen = render_text(&app, &unicode(), 80, 24);
    assert!(screen.contains("landing"));
    assert!(!screen.contains("pg-main"));
    assert_snapshot!("dashboard_narrow_projects", screen);

    update(&mut app, key(ratatui::crossterm::event::KeyCode::Tab));
    let screen = render_text(&app, &unicode(), 80, 24);
    assert!(screen.contains("pg-main"));
    assert_snapshot!("dashboard_narrow_resources", screen);
}

#[test]
fn dashboard_loading() {
    let mut app = logged_in_app();
    app.start();
    let screen = render_text(&app, &unicode(), 120, 40);
    assert!(screen.contains("Loading…"));
    assert_snapshot!(screen);
    assert_snapshot!(
        "dashboard_loading_small",
        render_text(&app, &unicode(), 80, 24)
    );
}

#[test]
fn dashboard_without_projects() {
    let mut app = logged_in_app();
    app.start();
    deliver(&mut app, Request::Projects, Payload::Projects(Vec::new()));
    deliver(
        &mut app,
        Request::UnassignedServices,
        Payload::UnassignedServices(Vec::new()),
    );
    let screen = render_text(&app, &unicode(), 120, 40);
    assert!(screen.contains("No projects yet"));
    assert_snapshot!(screen);
}

#[test]
fn dashboard_with_an_empty_project() {
    let mut app = loaded_dashboard();
    let mut empty = landing_detail();
    empty.web_services_list.clear();
    empty.databases_list.clear();
    empty.redis_list.clear();
    app.dashboard.details.insert(LANDING, empty);
    let screen = render_text(&app, &unicode(), 120, 40);
    assert!(screen.contains("No resources in this project."));
}

#[test]
fn dashboard_offline() {
    let mut app = loaded_dashboard();
    app.poller.refresh_all(app.now);
    update(&mut app, Action::Tick { now: at(4) });
    fail(
        &mut app,
        Request::Projects,
        FetchError::Offline {
            message: "down".to_string(),
        },
    );
    let screen = render_text(&app, &unicode(), 120, 40);
    assert!(screen.contains("offline"));
}

#[test]
fn ascii_mode_has_no_unicode_on_any_screen() {
    let mut loading = logged_in_app();
    loading.start();
    assert!(render_text(&loading, &ascii(), 80, 24).is_ascii());

    let login = App::new(session(false), None, at(0), (80, 24));
    assert!(render_text(&login, &ascii(), 80, 24).is_ascii());

    let mut help = loaded_dashboard();
    update(&mut help, char_key('?'));
    let screen = render_text(&help, &ascii(), 80, 24);
    assert!(screen.is_ascii(), "non-ASCII output:\n{screen}");
}

#[test]
fn the_latest_error_is_shown_in_the_status_line() {
    let mut app = loaded_dashboard();
    app.last_error = Some("Bad request".to_string());
    assert!(render_text(&app, &unicode(), 120, 40).contains("Bad request"));
}

#[test]
fn dashboard_ascii() {
    let app = loaded_dashboard();
    let screen = render_text(&app, &ascii(), 120, 40);
    assert!(screen.is_ascii(), "non-ASCII output:\n{screen}");
    assert_snapshot!(screen);
    assert_snapshot!("dashboard_ascii_small", render_text(&app, &ascii(), 80, 24));
}

#[test]
fn compact_header_under_twenty_rows() {
    let app = loaded_dashboard();
    let screen = render_text(&app, &unicode(), 120, 18);
    assert!(screen.contains("RunSite CLI v"));
    assert!(!screen.contains("█▀▄"));
    assert_snapshot!(screen);
}

#[test]
fn env_token_replaces_the_profile_name() {
    let mut app = loaded_dashboard();
    app.session.env_token = true;
    let screen = render_text(&app, &unicode(), 120, 40);
    assert!(screen.contains("env: RUNSITE_API_TOKEN"));
    assert!(!screen.contains("profile: default"));
}

#[test]
fn login_form() {
    let mut app = App::new(session(false), None, at(0), (80, 24));
    for character in "ada@example.com".chars() {
        update(&mut app, char_key(character));
    }
    let screen = render_text(&app, &unicode(), 80, 24);
    assert!(screen.contains("Email + password"));
    assert!(screen.contains("ada@example.com"));
    assert_snapshot!(screen);
    assert_snapshot!("login_form_wide", render_text(&app, &unicode(), 120, 40));
}

#[test]
fn login_after_a_revoked_key() {
    let mut app = loaded_dashboard();
    fail(
        &mut app,
        Request::Projects,
        FetchError::Unauthorized {
            message: "bad".to_string(),
        },
    );
    let screen = render_text(&app, &unicode(), 80, 24);
    assert!(screen.contains("Your API key is invalid or revoked"));
}

#[test]
fn too_small() {
    let app = loaded_dashboard();
    let screen = render_text(&app, &unicode(), 59, 20);
    assert!(screen.contains("Please enlarge the terminal window"));
    assert_snapshot!(screen);
}

#[test]
fn help_overlay() {
    let mut app = loaded_dashboard();
    update(&mut app, char_key('?'));
    let screen = render_text(&app, &unicode(), 80, 24);
    assert!(screen.contains("filter the focused list"));
    assert_snapshot!(screen);
}

#[test]
fn profile_picker() {
    let mut app = loaded_dashboard();
    update(&mut app, char_key('P'));
    update(
        &mut app,
        Action::ProfilesListed(vec![
            ProfileEntry {
                name: "default".to_string(),
                api_url: "https://api.runsite.app".to_string(),
                has_key: true,
                current_project_id: None,
            },
            ProfileEntry {
                name: "staging".to_string(),
                api_url: "https://staging.runsite.app".to_string(),
                has_key: false,
                current_project_id: None,
            },
        ]),
    );
    let screen = render_text(&app, &unicode(), 80, 24);
    assert!(screen.contains("(current)"));
    assert!(screen.contains("(no key)"));
    assert_snapshot!(screen);
}

#[test]
fn blocked_account() {
    let mut app = loaded_dashboard();
    fail(
        &mut app,
        Request::Projects,
        FetchError::Blocked {
            reason: "unpaid invoice".to_string(),
        },
    );
    let screen = render_text(&app, &unicode(), 80, 24);
    assert!(screen.contains("Your account is blocked"));
    assert!(screen.contains("unpaid invoice"));
    assert_snapshot!(screen);
}

fn loaded_service_detail(tab_key: char) -> App {
    let mut app = service_detail(tab_key);
    let mut api = service(API_SERVICE, "api", "running", Some(LANDING));
    api.max_instances = Some(3);
    deliver(
        &mut app,
        Request::Service(API_SERVICE),
        Payload::Service(Box::new(api)),
    );
    deliver(
        &mut app,
        Request::LatestDeployment(API_SERVICE),
        Payload::LatestDeployment(deployments().into_iter().next().map(Box::new)),
    );
    deliver(
        &mut app,
        Request::Deployments(API_SERVICE),
        Payload::Deployments(deployments()),
    );
    app
}

#[test]
fn service_overview() {
    let mut app = loaded_service_detail('1');
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
    let screen = render_text(&app, &unicode(), 120, 40);
    assert!(screen.contains("https://api-x.runsite.app"));
    assert!(screen.contains("1–3 (live 2)"));
    assert!(screen.contains("12.5%"));
    assert!(screen.contains("256 MB / 512 MB"));
    assert!(screen.contains("3f2a9c1 change 3f2a9c1d"));
    assert_snapshot!(screen);
    assert_snapshot!(
        "service_overview_small",
        render_text(&app, &unicode(), 80, 24)
    );
}

#[test]
fn a_stopped_service_shows_no_metrics() {
    let mut app = loaded_service_detail('1');
    deliver(
        &mut app,
        Request::Service(API_SERVICE),
        Payload::Service(Box::new(service(
            API_SERVICE,
            "api",
            "stopped",
            Some(LANDING),
        ))),
    );
    let screen = render_text(&app, &unicode(), 120, 40);
    assert!(screen.contains("CPU        —"));
    assert!(!screen.contains("(live"));
}

#[test]
fn service_logs() {
    let mut app = loaded_service_detail('2');
    deliver(
        &mut app,
        Request::Logs(API_SERVICE),
        Payload::Logs {
            body:
                "2026-09-23T10:00:01.5Z GET /health 200\n2026-09-23T10:00:02Z GET /api/users 500\n"
                    .to_string(),
            query: crate::tui::log_buffer::LogQuery {
                tail: 500,
                since: None,
            },
        },
    );
    let screen = render_text(&app, &unicode(), 120, 40);
    assert!(screen.contains("Showing logs from the newest instance"));
    assert!(screen.contains("10:00:02 GET /api/users 500"));
    assert_snapshot!(screen);
    assert_snapshot!("service_logs_small", render_text(&app, &unicode(), 80, 24));
}

#[test]
fn service_logs_waiting_message() {
    let mut app = loaded_service_detail('2');
    deliver(
        &mut app,
        Request::Logs(API_SERVICE),
        Payload::Logs {
            body: "Service is not ready yet. Try again shortly.".to_string(),
            query: crate::tui::log_buffer::LogQuery {
                tail: 500,
                since: None,
            },
        },
    );
    let screen = render_text(&app, &unicode(), 120, 40);
    assert!(screen.contains("Service is not ready yet."));
}

#[test]
fn service_deploys() {
    let app = loaded_service_detail('3');
    let screen = render_text(&app, &unicode(), 120, 40);
    assert!(screen.contains("live"));
    assert!(screen.contains("ready"));
    assert!(screen.contains("7b3e0f2"));
    assert_snapshot!(screen);
    assert_snapshot!(
        "service_deploys_small",
        render_text(&app, &unicode(), 80, 24)
    );
}

#[test]
fn deployment_detail() {
    let mut app = loaded_service_detail('3');
    update(&mut app, key(ratatui::crossterm::event::KeyCode::Enter));
    let screen = render_text(&app, &unicode(), 120, 40);
    assert!(screen.contains("Deployment 3f2a9c1"));
    assert!(screen.contains("Step 2/3 : RUN npm ci"));
    assert_snapshot!(screen);
}

#[test]
fn database_detail() {
    let mut app = loaded_dashboard();
    update(&mut app, key(ratatui::crossterm::event::KeyCode::Tab));
    for _ in 0..3 {
        update(&mut app, char_key('j'));
    }
    update(&mut app, key(ratatui::crossterm::event::KeyCode::Enter));
    deliver(
        &mut app,
        Request::Database(POSTGRES),
        Payload::Database(Box::new(postgres_detail())),
    );
    let screen = render_text(&app, &unicode(), 120, 40);
    assert!(screen.contains("postgresql"));
    assert!(screen.contains("pg-main.internal"));
    assert!(screen.contains("7.5%"));
    assert_snapshot!(screen);
}

#[test]
fn jumping_to_the_top_of_the_logs_shows_the_first_lines() {
    let mut app = loaded_service_detail('2');
    let body: String = (0..60)
        .map(|index| format!("2026-09-23T10:00:{:02}Z line {index}\n", index % 60))
        .collect();
    deliver(
        &mut app,
        Request::Logs(API_SERVICE),
        Payload::Logs {
            body,
            query: crate::tui::log_buffer::LogQuery {
                tail: 500,
                since: None,
            },
        },
    );
    update(&mut app, char_key('g'));
    let screen = render_text(&app, &unicode(), 80, 24);
    assert!(screen.contains("line 0 "), "{screen}");
}

#[test]
fn a_log_message_stays_visible_above_existing_lines() {
    let mut app = loaded_service_detail('2');
    deliver(
        &mut app,
        Request::Logs(API_SERVICE),
        Payload::Logs {
            body: "2026-09-23T10:00:01Z hello\n".to_string(),
            query: crate::tui::log_buffer::LogQuery {
                tail: 500,
                since: None,
            },
        },
    );
    if let crate::tui::app::Screen::ServiceDetail(detail) = &mut app.screen {
        detail.logs.status_message = Some("Service is not ready yet".to_string());
    }
    let screen = render_text(&app, &unicode(), 120, 40);
    assert!(screen.contains("Service is not ready yet"));
    assert!(screen.contains("hello"));
}

#[test]
fn confirm_modal() {
    let mut app = loaded_dashboard();
    update(&mut app, key(ratatui::crossterm::event::KeyCode::Tab));
    no_deployment_running(&mut app, API_SERVICE);
    update(&mut app, char_key('R'));
    let screen = render_text(&app, &unicode(), 80, 24);
    assert!(screen.contains("Restart api?"));
    assert!(screen.contains("y confirm"));
    assert_snapshot!(screen);
    assert_snapshot!("confirm_modal_wide", render_text(&app, &unicode(), 120, 40));
}

#[test]
fn disabled_actions_stay_in_the_hint_bar() {
    let mut app = loaded_dashboard();
    update(&mut app, key(ratatui::crossterm::event::KeyCode::Tab));
    update(&mut app, char_key('j'));
    update(&mut app, char_key('j'));
    let screen = render_text(&app, &unicode(), 120, 40);
    assert!(screen.contains("D deploy"));
    assert!(screen.contains("R restart"));
    assert!(screen.contains("S start"));
}

#[test]
fn wrapped_logs_keep_the_newest_line_visible() {
    let mut app = loaded_service_detail('2');
    let word = "x".repeat(40);
    let mut body: String = (0..20)
        .map(|index| format!("2026-09-23T10:00:{index:02}Z {word} {word} {word}\n"))
        .collect();
    body.push_str("2026-09-23T10:00:30Z newest-line\n");
    deliver(
        &mut app,
        Request::Logs(API_SERVICE),
        Payload::Logs {
            body,
            query: crate::tui::log_buffer::LogQuery {
                tail: 500,
                since: None,
            },
        },
    );
    let screen = render_text(&app, &unicode(), 80, 24);
    assert!(screen.contains("newest-line"), "{screen}");
}
