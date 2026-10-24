use super::actions::ActionKey;
use super::log_buffer::LogQuery;
use super::poller::Ticket;
use crate::api::{
    ApiKeyIdentity, Credentials, DatabaseDetail, Deployment, MetricsPoint, Project, ProjectDetail,
    ResourceMetrics, Service, User,
};
use crate::error::ApiError;
use chrono::{DateTime, Utc};
use ratatui::crossterm::event::KeyEvent;
use uuid::Uuid;

/// Everything the TUI fetches. Also the key the poller schedules by.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Request {
    CurrentUser,
    KeyScope,
    Projects,
    ProjectDetail(Uuid),
    UnassignedServices,
    Service(Uuid),
    /// `deployments?limit=1`, polled fast to notice a deploy in progress.
    LatestDeployment(Uuid),
    Metrics(Uuid),
    MetricsHistory(Uuid),
    Logs(Uuid),
    Deployments(Uuid),
    /// Service id, deployment id.
    Deployment(Uuid, Uuid),
    Database(Uuid),
}

/// A request that changes remote state. Only sent after `y` in the confirm modal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mutation {
    Deploy {
        service_id: Uuid,
    },
    Restart {
        service_id: Uuid,
    },
    StartService {
        service_id: Uuid,
    },
    StopService {
        service_id: Uuid,
    },
    Rollback {
        service_id: Uuid,
        deployment_id: Uuid,
    },
    StartDatabase {
        database_id: Uuid,
    },
    StopDatabase {
        database_id: Uuid,
    },
}

impl Mutation {
    /// The action key that asks for this mutation.
    pub fn action_key(&self) -> ActionKey {
        match self {
            Mutation::Deploy { .. } => ActionKey::Deploy,
            Mutation::Restart { .. } => ActionKey::Restart,
            Mutation::Rollback { .. } => ActionKey::Rollback,
            Mutation::StartService { .. }
            | Mutation::StopService { .. }
            | Mutation::StartDatabase { .. }
            | Mutation::StopDatabase { .. } => ActionKey::StartStop,
        }
    }

    /// Opens the confirm prompt: "Restart **api**?".
    pub fn verb(&self) -> &'static str {
        match self {
            Mutation::Deploy { .. } => "Deploy",
            Mutation::Restart { .. } => "Restart",
            Mutation::StartService { .. } | Mutation::StartDatabase { .. } => "Start",
            Mutation::StopService { .. } | Mutation::StopDatabase { .. } => "Stop",
            Mutation::Rollback { .. } => "Roll back",
        }
    }

    /// Toast shown once the API accepted the request.
    pub fn accepted_message(&self) -> &'static str {
        match self {
            Mutation::Deploy { .. } => "Deployment started",
            Mutation::Restart { .. } => "Restart requested",
            Mutation::StartService { .. } | Mutation::StartDatabase { .. } => "Start requested",
            Mutation::StopService { .. } | Mutation::StopDatabase { .. } => "Stop requested",
            Mutation::Rollback { .. } => "Rollback started",
        }
    }
}

#[derive(Debug)]
pub enum Payload {
    CurrentUser(User),
    KeyScope(ApiKeyIdentity),
    Projects(Vec<Project>),
    ProjectDetail(ProjectDetail),
    UnassignedServices(Vec<Service>),
    Service(Box<Service>),
    LatestDeployment(Option<Box<Deployment>>),
    Metrics(ResourceMetrics),
    MetricsHistory(Vec<MetricsPoint>),
    Logs { body: String, query: LogQuery },
    Deployments(Vec<Deployment>),
    Deployment(Box<Deployment>),
    Database(Box<DatabaseDetail>),
}

pub enum Action {
    Key(KeyEvent),
    Resize {
        width: u16,
        height: u16,
    },
    Tick {
        now: DateTime<Utc>,
    },
    /// `ticket` names the send this answers; see `Poller::accepts`.
    Loaded {
        ticket: Ticket,
        request: Request,
        result: Result<Payload, ApiError>,
    },
    /// Carries the logged-in email.
    LoggedIn {
        generation: u64,
        result: Result<String, ApiError>,
    },
    Mutated {
        generation: u64,
        mutation: Mutation,
        result: Result<(), ApiError>,
    },
    ProfilesListed(Vec<ProfileEntry>),
    /// SIGTERM, SIGHUP or the console window closing.
    Terminate,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileEntry {
    pub name: String,
    pub api_url: String,
    pub has_key: bool,
    pub current_project_id: Option<Uuid>,
}

pub enum Effect {
    Fetch {
        ticket: Ticket,
        request: Request,
        /// Where the log buffer wants to resume, for `Request::Logs`.
        log_query: Option<LogQuery>,
    },
    LogIn {
        generation: u64,
        credentials: Credentials,
    },
    Mutate {
        generation: u64,
        mutation: Mutation,
    },
    ListProfiles,
    SwitchProfile {
        profile: String,
    },
    Exit,
}
