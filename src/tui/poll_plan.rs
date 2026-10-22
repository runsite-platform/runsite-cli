//! What each screen keeps fresh, and how often. A pure function of the state:
//! the poller turns this plan into due requests.

use super::action::Request;
use super::app::Screen;
use super::dashboard::DashboardState;
use super::poller::Cadence;

pub fn desired_polls(screen: &Screen, dashboard: &DashboardState) -> Vec<(Request, Cadence)> {
    let mut polls = vec![
        (Request::CurrentUser, Cadence::Once),
        (Request::KeyScope, Cadence::Once),
    ];
    match screen {
        Screen::Login(_) | Screen::Blocked { .. } => return Vec::new(),
        Screen::Dashboard => polls.extend(dashboard.polls()),
        Screen::ServiceDetail(detail) => polls.extend(detail.polls()),
        Screen::DatabaseDetail(database) => polls.push(database.poll()),
    }
    polls
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::detail::{DetailTab, ServiceDetailState};
    use crate::tui::fixtures::{service, API_SERVICE, LANDING};
    use crate::tui::login::LoginState;

    fn requests(polls: &[(Request, Cadence)]) -> Vec<Request> {
        polls.iter().map(|(request, _)| *request).collect()
    }

    fn cadence_of(polls: &[(Request, Cadence)], wanted: Request) -> Option<Cadence> {
        polls
            .iter()
            .find(|(request, _)| *request == wanted)
            .map(|(_, cadence)| *cadence)
    }

    fn detail_screen(status: &str, tab: DetailTab) -> Screen {
        let api = service(API_SERVICE, "api", status, Some(LANDING));
        Screen::ServiceDetail(Box::new(ServiceDetailState::new(api, tab)))
    }

    #[test]
    fn nothing_is_polled_without_a_session() {
        let dashboard = DashboardState::new(None);
        let login = Screen::Login(LoginState::default());
        assert!(desired_polls(&login, &dashboard).is_empty());
        let blocked = Screen::Blocked {
            reason: String::new(),
        };
        assert!(desired_polls(&blocked, &dashboard).is_empty());
    }

    #[test]
    fn a_project_detail_waits_for_the_project_list() {
        let dashboard = DashboardState::new(Some(LANDING));
        let polls = desired_polls(&Screen::Dashboard, &dashboard);
        assert_eq!(
            requests(&polls),
            vec![
                Request::CurrentUser,
                Request::KeyScope,
                Request::Projects,
                Request::UnassignedServices,
            ]
        );
    }

    #[test]
    fn each_service_tab_polls_only_what_it_shows() {
        let dashboard = DashboardState::new(None);
        let logs = desired_polls(&detail_screen("running", DetailTab::Logs), &dashboard);
        assert!(requests(&logs).contains(&Request::Logs(API_SERVICE)));
        assert!(!requests(&logs).contains(&Request::Metrics(API_SERVICE)));

        let overview = desired_polls(&detail_screen("running", DetailTab::Overview), &dashboard);
        assert!(requests(&overview).contains(&Request::Metrics(API_SERVICE)));
        assert!(!requests(&overview).contains(&Request::Logs(API_SERVICE)));
    }

    #[test]
    fn a_service_in_transition_is_polled_every_second() {
        let dashboard = DashboardState::new(None);
        let restarting = desired_polls(
            &detail_screen("restarting", DetailTab::Overview),
            &dashboard,
        );
        assert_eq!(
            cadence_of(&restarting, Request::Service(API_SERVICE)),
            Some(Cadence::seconds(1))
        );
        let running = desired_polls(&detail_screen("running", DetailTab::Overview), &dashboard);
        assert_eq!(
            cadence_of(&running, Request::Service(API_SERVICE)),
            Some(Cadence::seconds(3))
        );
    }
}
