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
pub struct ProjectRow {
    pub choice: ProjectChoice,
    pub name: String,
    pub resource_count: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Resource {
    Service(ServiceInfo),
    Postgres(PostgresInProject),
    Redis(RedisInProject),
}

impl Resource {
    pub fn name(&self) -> &str {
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

fn matches_filter(name: &str, filter: &str) -> bool {
    filter.is_empty() || name.to_lowercase().contains(&filter.to_lowercase())
}

impl DashboardState {
    pub fn new(preferred_project: Option<Uuid>) -> Self {
        Self {
            selected_project: preferred_project.map(ProjectChoice::Project),
            ..Default::default()
        }
    }

    pub fn project_rows(&self) -> Vec<ProjectRow> {
        let mut rows: Vec<ProjectRow> = self
            .projects
            .iter()
            .flatten()
            .map(|project| {
                let summary = &project.service_summary;
                ProjectRow {
                    choice: ProjectChoice::Project(project.id),
                    name: project.name.clone(),
                    resource_count: summary.web_services + summary.postgresql + summary.redis,
                }
            })
            .collect();

        let unassigned_count = self.unassigned.as_ref().map_or(0, Vec::len);
        if unassigned_count > 0 {
            rows.push(ProjectRow {
                choice: ProjectChoice::Unassigned,
                name: "Unassigned".to_string(),
                resource_count: unassigned_count as i64,
            });
        }

        rows.retain(|row| matches_filter(&row.name, &self.project_filter));
        rows
    }

    pub fn selected_row_index(&self) -> Option<usize> {
        let selected = self.selected_project?;
        self.project_rows()
            .iter()
            .position(|row| row.choice == selected)
    }

    /// Keep the selection on a visible row: the first one when the previous
    /// choice disappeared or was filtered out.
    pub fn normalize_selection(&mut self) {
        if self.projects.is_none() {
            return;
        }
        let rows = self.project_rows();
        let still_visible = self
            .selected_project
            .is_some_and(|selected| rows.iter().any(|row| row.choice == selected));
        if !still_visible {
            self.selected_project = rows.first().map(|row| row.choice);
            self.resource_cursor = 0;
        }
        let resource_count = self.resources().len();
        if self.resource_cursor >= resource_count {
            self.resource_cursor = resource_count.saturating_sub(1);
        }
    }

    pub fn move_project(&mut self, delta: isize) {
        let rows = self.project_rows();
        if rows.is_empty() {
            return;
        }
        let current = self.selected_row_index().unwrap_or(0);
        let next = current.saturating_add_signed(delta).min(rows.len() - 1);
        if rows[next].choice != rows[current].choice || self.selected_project.is_none() {
            self.resource_cursor = 0;
        }
        self.selected_project = Some(rows[next].choice);
    }

    pub fn move_resource(&mut self, delta: isize) {
        let count = self.resources().len();
        if count == 0 {
            return;
        }
        self.resource_cursor = self
            .resource_cursor
            .saturating_add_signed(delta)
            .min(count - 1);
    }

    pub fn selected_project_name(&self) -> Option<String> {
        let selected = self.selected_project?;
        self.project_rows()
            .into_iter()
            .find(|row| row.choice == selected)
            .map(|row| row.name)
    }

    /// Web services, then Postgres, then Redis/Valkey.
    pub fn resources(&self) -> Vec<Resource> {
        let mut resources: Vec<Resource> = match self.selected_project {
            Some(ProjectChoice::Project(id)) => match self.details.get(&id) {
                Some(detail) => detail
                    .web_services_list
                    .iter()
                    .cloned()
                    .map(Resource::Service)
                    .chain(
                        detail
                            .databases_list
                            .iter()
                            .cloned()
                            .map(Resource::Postgres),
                    )
                    .chain(detail.redis_list.iter().cloned().map(Resource::Redis))
                    .collect(),
                None => Vec::new(),
            },
            Some(ProjectChoice::Unassigned) => self
                .unassigned
                .iter()
                .flatten()
                .cloned()
                .map(Resource::Service)
                .collect(),
            None => Vec::new(),
        };
        resources.retain(|resource| matches_filter(resource.name(), &self.resource_filter));
        resources
    }

    pub fn selected_resource(&self) -> Option<Resource> {
        self.resources().into_iter().nth(self.resource_cursor)
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
