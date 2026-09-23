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
