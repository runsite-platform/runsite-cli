//! What each screen keeps fresh, and how often. A pure function of the state:
//! the poller turns this plan into due requests.

use super::action::Request;
use super::app::Screen;
use super::dashboard::{DashboardState, Pane, ProjectChoice, Resource};
use super::detail::{DatabaseDetailState, DetailTab, ServiceDetailState};
use super::poller::Cadence;
use super::status::{database_look, deployment_in_progress};

pub fn desired_polls(screen: &Screen, dashboard: &DashboardState) -> Vec<(Request, Cadence)> {
    let mut polls = vec![
        (Request::CurrentUser, Cadence::Once),
        (Request::KeyScope, Cadence::Once),
    ];
    match screen {
        Screen::Login(_) | Screen::Blocked { .. } => return Vec::new(),
        Screen::Dashboard => polls.extend(dashboard_polls(dashboard)),
        Screen::ServiceDetail(detail) => polls.extend(service_polls(detail)),
        Screen::DatabaseDetail(database) => polls.push(database_poll(database)),
    }
    polls
}

fn dashboard_polls(dashboard: &DashboardState) -> Vec<(Request, Cadence)> {
    let mut polls = vec![
        (Request::Projects, Cadence::seconds(30)),
        // Needed on every dashboard view: the Unassigned row only exists
        // when this list has services without a project.
        (Request::UnassignedServices, Cadence::seconds(30)),
    ];
    // Wait for the list: a project saved in the config may no longer exist.
    if let (Some(_), Some(ProjectChoice::Project(id))) =
        (&dashboard.projects, dashboard.selected_project)
    {
        polls.push((Request::ProjectDetail(id), Cadence::seconds(10)));
    }
    if dashboard.focus == Pane::Resources {
        if let Some(Resource::Service(service)) = dashboard.selected_resource() {
            polls.push((Request::LatestDeployment(service.id), Cadence::seconds(3)));
        }
    }
    polls
}

fn database_poll(database: &DatabaseDetailState) -> (Request, Cadence) {
    let transitional = database
        .detail
        .as_ref()
        .is_some_and(|detail| database_look(&detail.status).transitional);
    let interval = if transitional { 1 } else { 5 };
    (
        Request::Database(database.database_id),
        Cadence::seconds(interval),
    )
}

fn service_polls(detail: &ServiceDetailState) -> Vec<(Request, Cadence)> {
    let id = detail.service_id;
    let fast = if detail.transitional() { 1 } else { 3 };
    let mut polls = vec![
        (Request::Service(id), Cadence::seconds(fast)),
        (Request::LatestDeployment(id), Cadence::seconds(fast)),
    ];
    let deployments_interval = if detail.deployment_in_progress() {
        3
    } else {
        10
    };
    match detail.tab {
        DetailTab::Overview => polls.extend([
            (Request::Metrics(id), Cadence::seconds(5)),
            (Request::MetricsHistory(id), Cadence::seconds(60)),
            (
                Request::Deployments(id),
                Cadence::seconds(deployments_interval),
            ),
        ]),
        DetailTab::Logs => polls.push((Request::Logs(id), Cadence::seconds(2))),
        DetailTab::Deploys => polls.push((
            Request::Deployments(id),
            Cadence::seconds(deployments_interval),
        )),
    }
    if let Some(view) = &detail.deployment_view {
        let cadence = match &view.detail {
            Some(deployment) if !deployment_in_progress(&deployment.status) => Cadence::Once,
            _ => Cadence::seconds(3),
        };
        polls.push((Request::Deployment(id, view.id), cadence));
    }
    polls
}

#[cfg(test)]
mod tests {
    use super::*;
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
