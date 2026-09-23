use super::log_buffer::LogBuffer;
use super::status::{deployment_in_progress, service_look};
use crate::api::{DatabaseDetail, DeploymentInfo, MetricsPoint, ResourceMetrics, ServiceInfo};
use uuid::Uuid;

/// Commit prefix, or the deployment id prefix for image deployments.
pub fn short_ref(deployment: &DeploymentInfo) -> String {
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
    pub detail: Option<DeploymentInfo>,
    /// First build-log line shown.
    pub scroll: usize,
}

#[derive(Debug)]
pub struct ServiceDetailState {
    pub service_id: Uuid,
    /// From the dashboard row, until the service itself loads.
    pub name: String,
    pub service: Option<ServiceInfo>,
    /// Newest deployment, polled fast to notice a deploy or rollback in progress.
    pub latest_deployment: Option<DeploymentInfo>,
    pub metrics: Option<ResourceMetrics>,
    pub history: Vec<MetricsPoint>,
    pub logs: LogBuffer,
    pub deployments: Option<Vec<DeploymentInfo>>,
    pub deploy_cursor: usize,
    pub tab: DetailTab,
    pub deployment_view: Option<DeploymentView>,
}

impl ServiceDetailState {
    pub fn new(service: ServiceInfo, tab: DetailTab) -> Self {
        Self {
            service_id: service.id,
            name: service.name.clone(),
            service: Some(service),
            latest_deployment: None,
            metrics: None,
            history: Vec::new(),
            logs: LogBuffer::default(),
            deployments: None,
            deploy_cursor: 0,
            tab,
            deployment_view: None,
        }
    }

    pub fn status(&self) -> Option<&str> {
        self.service.as_ref().map(|service| service.status.as_str())
    }

    pub fn is_running(&self) -> bool {
        self.status() == Some("running")
    }

    /// A deploy or rollback is running even while the service stays `running`.
    pub fn deployment_in_progress(&self) -> bool {
        self.latest_deployment
            .as_ref()
            .is_some_and(|deployment| deployment_in_progress(&deployment.status))
            || self
                .deployments
                .iter()
                .flatten()
                .any(|deployment| deployment_in_progress(&deployment.status))
    }

    pub fn transitional(&self) -> bool {
        self.status()
            .is_some_and(|status| service_look(status).transitional)
            || self.deployment_in_progress()
    }

    pub fn live_deployment(&self) -> Option<&DeploymentInfo> {
        self.deployments
            .iter()
            .flatten()
            .find(|deployment| deployment.is_live)
    }

    pub fn selected_deployment(&self) -> Option<&DeploymentInfo> {
        self.deployments.as_ref()?.get(self.deploy_cursor)
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
