use super::action::{Payload, Request};
use super::action_gate::{plain_confirm, service_confirm, Confirm};
use super::actions::{
    database_mutation, rollback_mutation, service_mutation, ActionKey, DeploymentActivity,
    Unavailable,
};
use super::log_buffer::{LogBuffer, LogQuery};
use super::poller::Cadence;
use crate::api::{
    DatabaseDetail, Deployment, MetricsPoint, ResourceMetrics, Service, ServiceStatus, Status,
};
use crate::error::ApiError;
use chrono::{DateTime, Utc};
use uuid::Uuid;

/// What a 404 on one of a screen's requests means for that screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Vanished {
    /// Not one of this screen's requests.
    Unrelated,
    /// Part of the screen is gone and was closed; the screen stays.
    Part,
    /// The resource the screen shows is gone.
    Whole,
}

/// Commit prefix, or the deployment id prefix for image deployments.
pub fn short_ref(deployment: &Deployment) -> String {
    match &deployment.commit_sha {
        Some(sha) => sha.chars().take(7).collect(),
        None => deployment.id.to_string().chars().take(8).collect(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DetailTab {
    Overview,
    Logs,
    Deploys,
}

impl DetailTab {
    pub const ALL: [DetailTab; 3] = [DetailTab::Overview, DetailTab::Logs, DetailTab::Deploys];

    pub fn title(self) -> &'static str {
        match self {
            DetailTab::Overview => "Overview",
            DetailTab::Logs => "Logs",
            DetailTab::Deploys => "Deploys",
        }
    }

    pub fn from_digit(digit: char) -> Option<Self> {
        match digit {
            '1' => Some(DetailTab::Overview),
            '2' => Some(DetailTab::Logs),
            '3' => Some(DetailTab::Deploys),
            _ => None,
        }
    }

    pub fn next(self) -> Self {
        match self {
            DetailTab::Overview => DetailTab::Logs,
            DetailTab::Logs => DetailTab::Deploys,
            DetailTab::Deploys => DetailTab::Overview,
        }
    }

    pub fn previous(self) -> Self {
        self.next().next()
    }
}

#[derive(Debug)]
pub struct DeploymentView {
    pub id: Uuid,
    pub detail: Option<Deployment>,
    /// First build-log line shown.
    pub scroll: usize,
}

#[derive(Debug)]
pub struct ServiceDetailState {
    pub service_id: Uuid,
    /// From the dashboard row, until the service itself loads.
    pub name: String,
    pub service: Option<Service>,
    /// Newest deployment, polled fast to notice a deploy or rollback in progress.
    pub latest_deployment: Option<Deployment>,
    /// True once the latest deployment or the deployment list has answered.
    pub deployments_checked: bool,
    pub metrics: Option<ResourceMetrics>,
    pub history: Vec<MetricsPoint>,
    pub logs: LogBuffer,
    pub deployments: Option<Vec<Deployment>>,
    pub deploy_cursor: usize,
    pub tab: DetailTab,
    pub deployment_view: Option<DeploymentView>,
}

impl ServiceDetailState {
    pub fn new(service: Service, tab: DetailTab) -> Self {
        Self {
            service_id: service.id,
            name: service.name.clone(),
            service: Some(service),
            latest_deployment: None,
            deployments_checked: false,
            metrics: None,
            history: Vec::new(),
            logs: LogBuffer::default(),
            deployments: None,
            deploy_cursor: 0,
            tab,
            deployment_view: None,
        }
    }

    pub fn status(&self) -> Option<&ServiceStatus> {
        self.service.as_ref().map(|service| &service.status)
    }

    pub fn is_running(&self) -> bool {
        self.status() == Some(&ServiceStatus::Running)
    }

    /// A deploy or rollback is running even while the service stays `running`.
    pub fn deployment_activity(&self) -> DeploymentActivity {
        if self.deployment_in_progress() {
            DeploymentActivity::Running
        } else if self.deployments_checked {
            DeploymentActivity::Idle
        } else {
            DeploymentActivity::Unchecked
        }
    }

    pub fn deployment_in_progress(&self) -> bool {
        self.latest_deployment
            .as_ref()
            .is_some_and(|deployment| deployment.status.is_transitional())
            || self
                .deployments
                .iter()
                .flatten()
                .any(|deployment| deployment.status.is_transitional())
    }

    pub fn transitional(&self) -> bool {
        self.status().is_some_and(Status::is_transitional) || self.deployment_in_progress()
    }

    pub fn live_deployment(&self) -> Option<&Deployment> {
        self.deployments
            .iter()
            .flatten()
            .find(|deployment| deployment.is_live)
    }

    pub fn selected_deployment(&self) -> Option<&Deployment> {
        self.deployments.as_ref()?.get(self.deploy_cursor)
    }

    /// What this screen keeps fresh, and how often.
    pub fn polls(&self) -> Vec<(Request, Cadence)> {
        let id = self.service_id;
        let fast = if self.transitional() { 1 } else { 3 };
        let mut polls = vec![
            (Request::Service(id), Cadence::seconds(fast)),
            (Request::LatestDeployment(id), Cadence::seconds(fast)),
        ];
        let deployments = (
            Request::Deployments(id),
            Cadence::seconds(if self.deployment_in_progress() { 3 } else { 10 }),
        );
        match self.tab {
            DetailTab::Overview => polls.extend([
                (Request::Metrics(id), Cadence::seconds(5)),
                (Request::MetricsHistory(id), Cadence::seconds(60)),
                deployments,
            ]),
            DetailTab::Logs => polls.push((Request::Logs(id), Cadence::seconds(2))),
            DetailTab::Deploys => polls.push(deployments),
        }
        if let Some(view) = &self.deployment_view {
            let cadence = match &view.detail {
                Some(deployment) if !deployment.status.is_transitional() => Cadence::Once,
                _ => Cadence::seconds(3),
            };
            polls.push((Request::Deployment(id, view.id), cadence));
        }
        polls
    }

    /// True when `request` reads the service this screen shows.
    fn concerns(&self, request: Request) -> bool {
        match request {
            Request::Service(id)
            | Request::LatestDeployment(id)
            | Request::Metrics(id)
            | Request::MetricsHistory(id)
            | Request::Logs(id)
            | Request::Deployments(id)
            | Request::Deployment(id, _) => id == self.service_id,
            _ => false,
        }
    }

    /// Called as `request` is sent: the log window to ask for, if it reads logs.
    pub fn start_fetch(&mut self, request: Request) -> Option<LogQuery> {
        match request {
            Request::Logs(id) if id == self.service_id => Some(self.logs.start_fetch()),
            _ => None,
        }
    }

    /// True when the screen shows `error` in place, instead of the status line.
    pub fn fetch_failed(&mut self, request: Request, error: &ApiError) -> bool {
        matches!(request, Request::Logs(id) if id == self.service_id)
            && self.logs.fetch_failed(error)
    }

    /// What `key` would do here. `None` when the key means nothing on this
    /// screen or tab, `Err` when it is disabled.
    pub fn resolve_action(
        &self,
        key: ActionKey,
        key_scope: Option<&str>,
    ) -> Option<Result<Confirm, Unavailable>> {
        let service = self.service.as_ref()?;
        if key != ActionKey::Rollback {
            return Some(
                service_mutation(key, service, self.deployment_activity(), key_scope)?
                    .map(|mutation| service_confirm(mutation, service)),
            );
        }
        let target = match &self.deployment_view {
            Some(view) => view.detail.as_ref()?,
            None if self.tab == DetailTab::Deploys => self.selected_deployment()?,
            None => return None,
        };
        let mutation = rollback_mutation(
            service,
            target,
            self.deployments.as_deref().unwrap_or_default(),
            self.latest_deployment.as_ref(),
            key_scope,
        );
        Some(mutation.map(|mutation| Confirm {
            mutation,
            target: service.name.clone(),
            suffix: format!(" to {}?", short_ref(target)),
        }))
    }

    /// Takes in a reply to one of this screen's requests; any other is handed back.
    pub fn apply(
        &mut self,
        request: Request,
        payload: Payload,
        now: DateTime<Utc>,
    ) -> Option<Payload> {
        if !self.concerns(request) {
            return Some(payload);
        }
        match payload {
            Payload::Service(service) => {
                self.name = service.name.clone();
                self.service = Some(*service);
            }
            Payload::LatestDeployment(deployment) => {
                self.latest_deployment = deployment.map(|deployment| *deployment);
                self.deployments_checked = true;
            }
            Payload::Metrics(metrics) => self.metrics = Some(metrics),
            Payload::MetricsHistory(points) => self.history = points,
            Payload::Logs { body, query } => self.logs.receive(query, &body, now),
            Payload::Deployments(deployments) => {
                self.deploy_cursor = self.deploy_cursor.min(deployments.len().saturating_sub(1));
                self.deployments = Some(deployments);
                self.deployments_checked = true;
            }
            Payload::Deployment(deployment) => {
                if let Some(view) = &mut self.deployment_view {
                    if view.id == deployment.id {
                        view.detail = Some(*deployment);
                    }
                }
            }
            other => return Some(other),
        }
        None
    }

    pub fn vanished(&mut self, request: Request) -> Vanished {
        if !self.concerns(request) {
            return Vanished::Unrelated;
        }
        match request {
            Request::Deployment(..) => {
                if self.deployment_view.take().is_some() {
                    Vanished::Part
                } else {
                    Vanished::Unrelated
                }
            }
            _ => Vanished::Whole,
        }
    }
}

#[derive(Debug)]
pub struct DatabaseDetailState {
    pub database_id: Uuid,
    pub name: String,
    /// The detail endpoint has no version; it comes from the project list.
    pub version: Option<String>,
    pub detail: Option<DatabaseDetail>,
}

impl DatabaseDetailState {
    pub fn poll(&self) -> (Request, Cadence) {
        let transitional = self
            .detail
            .as_ref()
            .is_some_and(|detail| detail.status.is_transitional());
        (
            Request::Database(self.database_id),
            Cadence::seconds(if transitional { 1 } else { 5 }),
        )
    }

    /// `S` starts or stops the database once its status is known.
    pub fn resolve_action(
        &self,
        key: ActionKey,
        key_scope: Option<&str>,
    ) -> Option<Result<Confirm, Unavailable>> {
        if key != ActionKey::StartStop {
            return None;
        }
        let detail = self.detail.as_ref()?;
        Some(
            database_mutation(self.database_id, &detail.status, key_scope)
                .map(|mutation| plain_confirm(mutation, &self.name)),
        )
    }

    /// Takes in a reply to this screen's request; any other is handed back.
    pub fn apply(&mut self, request: Request, payload: Payload) -> Option<Payload> {
        match payload {
            Payload::Database(detail) if request == Request::Database(self.database_id) => {
                self.detail = Some(*detail);
                None
            }
            other => Some(other),
        }
    }

    pub fn vanished(&self, request: Request) -> Vanished {
        if request == Request::Database(self.database_id) {
            Vanished::Whole
        } else {
            Vanished::Unrelated
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::fixtures::{deployment, service, API_SERVICE, DEPLOYMENT_OLD, LANDING};

    fn detail() -> ServiceDetailState {
        let api = service(API_SERVICE, "api", "running", Some(LANDING));
        ServiceDetailState::new(api, DetailTab::Overview)
    }

    #[test]
    fn a_reply_for_another_screen_is_handed_back() {
        let mut detail = detail();
        let handed_back =
            detail.apply(Request::Projects, Payload::Projects(Vec::new()), Utc::now());
        assert!(matches!(handed_back, Some(Payload::Projects(_))));
        assert!(detail
            .apply(
                Request::Deployments(API_SERVICE),
                Payload::Deployments(Vec::new()),
                Utc::now()
            )
            .is_none());
        assert!(detail.deployments_checked);
    }

    #[test]
    fn a_reply_about_another_service_is_handed_back() {
        let mut detail = detail();
        let other_service = Uuid::from_u128(0xfeed);

        let handed_back = detail.apply(
            Request::Deployments(other_service),
            Payload::Deployments(Vec::new()),
            Utc::now(),
        );

        assert!(handed_back.is_some());
        assert!(!detail.deployments_checked);
        assert_eq!(
            detail.vanished(Request::Service(other_service)),
            Vanished::Unrelated
        );
    }

    #[test]
    fn a_vanished_deployment_closes_only_its_view() {
        let mut detail = detail();
        detail.deployment_view = Some(DeploymentView {
            id: DEPLOYMENT_OLD,
            detail: Some(deployment(DEPLOYMENT_OLD, "running", false, "7b3e0f2a")),
            scroll: 0,
        });

        let vanished = detail.vanished(Request::Deployment(API_SERVICE, DEPLOYMENT_OLD));

        assert_eq!(vanished, Vanished::Part);
        assert!(detail.deployment_view.is_none());
        assert_eq!(
            detail.vanished(Request::Service(API_SERVICE)),
            Vanished::Whole
        );
        assert_eq!(detail.vanished(Request::Projects), Vanished::Unrelated);
    }
}
