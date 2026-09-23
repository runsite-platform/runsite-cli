use crate::api::{
    DeploymentInfo, PostgresInProject, ProjectDetail, ProjectSummary, RedisInProject, ServiceInfo,
};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Pane {
    #[default]
    Projects,
    Resources,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectChoice {
    Project(Uuid),
    /// Web services without a project.
    Unassigned,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProjectRow<'a> {
    pub choice: ProjectChoice,
    pub name: &'a str,
    pub resource_count: i64,
}

/// A row of the Resources pane, borrowed from the loaded project data.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Resource<'a> {
    Service(&'a ServiceInfo),
    Postgres(&'a PostgresInProject),
    Redis(&'a RedisInProject),
}

impl<'a> Resource<'a> {
    pub fn name(&self) -> &'a str {
        match self {
            Resource::Service(service) => &service.name,
            Resource::Postgres(database) => &database.name,
            Resource::Redis(database) => &database.name,
        }
    }
}

#[derive(Debug, Default)]
pub struct DashboardState {
    /// `None` until the first response arrives.
    pub projects: Option<Vec<ProjectSummary>>,
    pub unassigned: Option<Vec<ServiceInfo>>,
    pub details: HashMap<Uuid, ProjectDetail>,
    pub selected_project: Option<ProjectChoice>,
    pub resource_cursor: usize,
    pub focus: Pane,
    pub project_filter: String,
    pub resource_filter: String,
    /// The pane whose filter is being typed into, if any.
    pub editing_filter: Option<Pane>,
    /// Newest deployment per service, polled for the selected service so that
    /// `D` and `R` can be refused while a deployment is in progress.
    pub latest_deployments: HashMap<Uuid, Option<DeploymentInfo>>,
}

/// Case-insensitive substring match; `filter` is already lowercase.
fn matches_filter(name: &str, filter: &str) -> bool {
    filter.is_empty() || name.to_lowercase().contains(filter)
}

impl DashboardState {
    pub fn new(preferred_project: Option<Uuid>) -> Self {
        Self {
            selected_project: preferred_project.map(ProjectChoice::Project),
            ..Default::default()
        }
    }

    fn project_row_iter(&self) -> impl Iterator<Item = ProjectRow<'_>> {
        let filter = self.project_filter.to_lowercase();
        let unassigned_count = self.unassigned.as_ref().map_or(0, Vec::len);
        let projects = self.projects.iter().flatten().map(|project| {
            let summary = &project.service_summary;
            ProjectRow {
                choice: ProjectChoice::Project(project.id),
                name: &project.name,
                resource_count: summary.web_services + summary.postgresql + summary.redis,
            }
        });
        let unassigned = (unassigned_count > 0).then_some(ProjectRow {
            choice: ProjectChoice::Unassigned,
            name: "Unassigned",
            resource_count: unassigned_count as i64,
        });
        projects
            .chain(unassigned)
            .filter(move |row| matches_filter(row.name, &filter))
    }

    pub fn project_rows(&self) -> Vec<ProjectRow<'_>> {
        self.project_row_iter().collect()
    }

    pub fn selected_row_index(&self) -> Option<usize> {
        let selected = self.selected_project?;
        self.project_row_iter()
            .position(|row| row.choice == selected)
    }

    /// Keep the selection on a visible row: the first one when the previous
    /// choice disappeared or was filtered out.
    pub fn normalize_selection(&mut self) {
        if self.projects.is_none() {
            return;
        }
        if self.selected_row_index().is_none() {
            let first = self.project_row_iter().next().map(|row| row.choice);
            self.selected_project = first;
            self.resource_cursor = 0;
        }
        let resource_count = self.resource_iter().count();
        if self.resource_cursor >= resource_count {
            self.resource_cursor = resource_count.saturating_sub(1);
        }
    }

    pub fn move_project(&mut self, delta: isize) {
        let choices: Vec<ProjectChoice> = self.project_row_iter().map(|row| row.choice).collect();
        if choices.is_empty() {
            return;
        }
        let current = self.selected_row_index().unwrap_or(0);
        let next = current.saturating_add_signed(delta).min(choices.len() - 1);
        if choices[next] != choices[current] || self.selected_project.is_none() {
            self.resource_cursor = 0;
        }
        self.selected_project = Some(choices[next]);
    }

    pub fn move_resource(&mut self, delta: isize) {
        let count = self.resource_iter().count();
        if count == 0 {
            return;
        }
        self.resource_cursor = self
            .resource_cursor
            .saturating_add_signed(delta)
            .min(count - 1);
    }

    pub fn selected_project_name(&self) -> Option<&str> {
        let selected = self.selected_project?;
        self.project_row_iter()
            .find(|row| row.choice == selected)
            .map(|row| row.name)
    }

    /// Web services, then Postgres, then Redis/Valkey.
    fn resource_iter(&self) -> impl Iterator<Item = Resource<'_>> {
        let filter = self.resource_filter.to_lowercase();
        let (services, databases, redis): (
            &[ServiceInfo],
            &[PostgresInProject],
            &[RedisInProject],
        ) = match self.selected_project {
            Some(ProjectChoice::Project(id)) => match self.details.get(&id) {
                Some(detail) => (
                    &detail.web_services_list,
                    &detail.databases_list,
                    &detail.redis_list,
                ),
                None => (&[], &[], &[]),
            },
            Some(ProjectChoice::Unassigned) => {
                (self.unassigned.as_deref().unwrap_or_default(), &[], &[])
            }
            None => (&[], &[], &[]),
        };
        services
            .iter()
            .map(Resource::Service)
            .chain(databases.iter().map(Resource::Postgres))
            .chain(redis.iter().map(Resource::Redis))
            .filter(move |resource| matches_filter(resource.name(), &filter))
    }

    pub fn resources(&self) -> Vec<Resource<'_>> {
        self.resource_iter().collect()
    }

    pub fn selected_resource(&self) -> Option<Resource<'_>> {
        self.resource_iter().nth(self.resource_cursor)
    }

    /// True while the selected project's resources have never been loaded.
    pub fn resources_loading(&self) -> bool {
        match self.selected_project {
            Some(ProjectChoice::Project(id)) => !self.details.contains_key(&id),
            Some(ProjectChoice::Unassigned) => self.unassigned.is_none(),
            None => self.projects.is_none(),
        }
    }

    pub fn filter_mut(&mut self, pane: Pane) -> &mut String {
        match pane {
            Pane::Projects => &mut self.project_filter,
            Pane::Resources => &mut self.resource_filter,
        }
    }

    pub fn filter(&self, pane: Pane) -> &str {
        match pane {
            Pane::Projects => &self.project_filter,
            Pane::Resources => &self.resource_filter,
        }
    }
}
