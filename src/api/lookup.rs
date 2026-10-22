//! Turn what the user typed (an ID or a name) into one resource, the same way
//! for every kind: services, databases and projects.

use super::{ApiClient, ManagedDatabase, Project, Service};
use anyhow::Result;
use std::fmt;
use uuid::Uuid;

/// A resource the user can refer to by ID or by name.
pub trait Named {
    /// Singular noun used in messages: "service", "database".
    const KIND: &'static str;
    /// Command that lists every resource of this kind, for hints.
    const LIST_COMMAND: &'static str;
    /// Whether names are looked up inside the selected project.
    const PROJECT_SCOPED: bool;

    fn id(&self) -> Uuid;
    fn name(&self) -> &str;
    fn project_id(&self) -> Option<Uuid>;
}

impl Named for Service {
    const KIND: &'static str = "service";
    const LIST_COMMAND: &'static str = "runsite service list --all";
    const PROJECT_SCOPED: bool = true;

    fn id(&self) -> Uuid {
        self.id
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn project_id(&self) -> Option<Uuid> {
        self.project_id
    }
}

impl Named for ManagedDatabase {
    const KIND: &'static str = "database";
    const LIST_COMMAND: &'static str = "runsite db list --all";
    const PROJECT_SCOPED: bool = true;

    fn id(&self) -> Uuid {
        self.id
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn project_id(&self) -> Option<Uuid> {
        self.project_id
    }
}

impl Named for Project {
    const KIND: &'static str = "project";
    const LIST_COMMAND: &'static str = "runsite project list";
    const PROJECT_SCOPED: bool = false;

    fn id(&self) -> Uuid {
        self.id
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn project_id(&self) -> Option<Uuid> {
        None
    }
}

/// Which part of the workspace a list asks the API for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectScope {
    Everything,
    Project(Uuid),
    WithoutProject,
}

impl ProjectScope {
    /// The selected project, or everything when none is selected.
    pub fn selected(project_id: Option<Uuid>) -> Self {
        project_id.map_or(Self::Everything, Self::Project)
    }

    /// The `project_id` query parameter for this scope, if any.
    pub fn query(self) -> Option<(&'static str, String)> {
        match self {
            Self::Everything => None,
            Self::Project(id) => Some(("project_id", id.to_string())),
            Self::WithoutProject => Some(("project_id", "none".to_string())),
        }
    }
}

/// True when `resource` sits in `selected_project`, or when none is selected.
pub fn belongs_to_project<T: Named>(resource: &T, selected_project: Option<Uuid>) -> bool {
    match selected_project {
        Some(selected) => resource.project_id() == Some(selected),
        None => true,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub name: String,
    pub id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LookupError {
    NotFound {
        kind: &'static str,
        reference: String,
    },
    /// The name exists, but outside the selected project.
    OutsideProject {
        kind: &'static str,
        reference: String,
        list_command: &'static str,
        owner_project_id: Option<Uuid>,
        owner_project_name: Option<String>,
    },
    Ambiguous {
        kind: &'static str,
        reference: String,
        matches: Vec<Candidate>,
    },
    /// Nothing was named and nothing is in scope.
    NoneAvailable { kind: &'static str },
    /// Nothing was named and there is more than one to choose from.
    NameRequired {
        kind: &'static str,
        matches: Vec<Candidate>,
    },
}

fn write_candidates(out: &mut fmt::Formatter<'_>, matches: &[Candidate]) -> fmt::Result {
    for candidate in matches {
        write!(out, "\n  {} ({})", candidate.name, candidate.id)?;
    }
    Ok(())
}

impl fmt::Display for LookupError {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound { kind, reference } => write!(out, "{kind} '{reference}' not found"),
            Self::OutsideProject {
                kind,
                reference,
                owner_project_name: Some(project),
                ..
            } => write!(
                out,
                "{kind} '{reference}' belongs to project '{project}'. Switch with: runsite project use \"{project}\""
            ),
            Self::OutsideProject {
                kind,
                reference,
                list_command,
                ..
            } => write!(
                out,
                "{kind} '{reference}' is not in the current project. See them all with: {list_command}"
            ),
            Self::Ambiguous {
                kind,
                reference,
                matches,
            } => {
                write!(out, "multiple {kind}s named '{reference}'. Use the ID instead:")?;
                write_candidates(out, matches)
            }
            Self::NoneAvailable { kind } => write!(out, "no {kind}s found"),
            Self::NameRequired { kind, matches } => {
                write!(out, "multiple {kind}s found. Specify a {kind} name:")?;
                write_candidates(out, matches)
            }
        }
    }
}

impl std::error::Error for LookupError {}

fn candidates<'a, T: Named + 'a>(resources: impl Iterator<Item = &'a T>) -> Vec<Candidate> {
    resources
        .map(|resource| Candidate {
            name: resource.name().to_string(),
            id: resource.id(),
        })
        .collect()
}

/// Pick the resource `reference` names. An ID matches anywhere; a name only
/// inside `selected_project`; no reference means the only one in scope.
pub fn pick<T: Named>(
    resources: Vec<T>,
    reference: Option<&str>,
    selected_project: Option<Uuid>,
) -> Result<T, LookupError> {
    let scope = selected_project.filter(|_| T::PROJECT_SCOPED);

    if let Some(id) = reference.and_then(|text| Uuid::parse_str(text).ok()) {
        return resources
            .into_iter()
            .find(|resource| resource.id() == id)
            .ok_or_else(|| LookupError::NotFound {
                kind: T::KIND,
                reference: id.to_string(),
            });
    }

    let (mut in_scope, outside): (Vec<T>, Vec<T>) = resources
        .into_iter()
        .partition(|resource| belongs_to_project(resource, scope));

    let Some(name) = reference else {
        return match in_scope.len() {
            0 => Err(LookupError::NoneAvailable { kind: T::KIND }),
            1 => Ok(in_scope.remove(0)),
            _ => Err(LookupError::NameRequired {
                kind: T::KIND,
                matches: candidates(in_scope.iter()),
            }),
        };
    };

    in_scope.retain(|resource| resource.name() == name);
    match in_scope.len() {
        1 => Ok(in_scope.remove(0)),
        0 => Err(
            match outside.iter().find(|resource| resource.name() == name) {
                Some(elsewhere) => LookupError::OutsideProject {
                    kind: T::KIND,
                    reference: name.to_string(),
                    list_command: T::LIST_COMMAND,
                    owner_project_id: elsewhere.project_id(),
                    owner_project_name: None,
                },
                None => LookupError::NotFound {
                    kind: T::KIND,
                    reference: name.to_string(),
                },
            },
        ),
        _ => Err(LookupError::Ambiguous {
            kind: T::KIND,
            reference: name.to_string(),
            matches: candidates(in_scope.iter()),
        }),
    }
}

impl ApiClient {
    /// `pick` within the selected project, naming the owning project when the
    /// name only exists elsewhere.
    pub(super) async fn find<T: Named>(
        &self,
        resources: Vec<T>,
        reference: Option<&str>,
    ) -> Result<T> {
        match pick(resources, reference, self.current_project_id()) {
            Ok(resource) => Ok(resource),
            Err(LookupError::OutsideProject {
                kind,
                reference,
                list_command,
                owner_project_id: Some(owner),
                owner_project_name: None,
            }) => Err(LookupError::OutsideProject {
                kind,
                reference,
                list_command,
                owner_project_id: Some(owner),
                owner_project_name: self.project_name(owner).await,
            }
            .into()),
            Err(error) => Err(error.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROJECT: &str = "ca83c4a9-1517-4fe3-93aa-bbdab4eaee23";
    const OTHER_PROJECT: &str = "8d5b5e8e-2348-4a29-8c05-bcfa1e599ef2";

    fn uuid(text: &str) -> Uuid {
        Uuid::parse_str(text).unwrap()
    }

    fn database(id_suffix: u8, name: &str, project: Option<&str>) -> ManagedDatabase {
        ManagedDatabase {
            id: uuid(&format!("3f2a1b4c-0000-4000-8000-0000000000{id_suffix:02}")),
            kind: "postgresql".to_string(),
            name: name.to_string(),
            status: crate::api::DatabaseStatus::Running,
            plan_name: None,
            storage_limit_gb: None,
            project_id: project.map(uuid),
        }
    }

    fn selected() -> Option<Uuid> {
        Some(uuid(PROJECT))
    }

    #[test]
    fn a_name_resolves_inside_the_selected_project() {
        let picked = pick(
            vec![
                database(1, "main", Some(OTHER_PROJECT)),
                database(2, "main", Some(PROJECT)),
            ],
            Some("main"),
            selected(),
        )
        .unwrap();
        assert_eq!(picked.id, database(2, "main", None).id);
    }

    #[test]
    fn an_id_resolves_in_any_project() {
        let wanted = database(1, "main", Some(OTHER_PROJECT));
        let picked = pick(
            vec![wanted],
            Some(&database(1, "", None).id.to_string()),
            selected(),
        );
        assert_eq!(picked.unwrap().name, "main");
    }

    #[test]
    fn a_name_in_another_project_is_reported_with_its_owner() {
        let error = pick(
            vec![database(1, "main", Some(OTHER_PROJECT))],
            Some("main"),
            selected(),
        )
        .unwrap_err();
        assert!(matches!(
            error,
            LookupError::OutsideProject { owner_project_id: Some(owner), .. } if owner == uuid(OTHER_PROJECT)
        ));
    }

    #[test]
    fn a_shared_name_lists_the_ids() {
        let error = pick(
            vec![database(1, "pg", None), database(2, "pg", None)],
            Some("pg"),
            None,
        )
        .unwrap_err();
        assert!(matches!(&error, LookupError::Ambiguous { matches, .. } if matches.len() == 2));
        assert!(error
            .to_string()
            .starts_with("multiple databases named 'pg'. Use the ID instead:\n  pg ("));
    }

    #[test]
    fn no_reference_picks_the_only_one_in_scope() {
        let picked = pick(
            vec![
                database(1, "other", Some(OTHER_PROJECT)),
                database(2, "main", Some(PROJECT)),
            ],
            None,
            selected(),
        )
        .unwrap();
        assert_eq!(picked.name, "main");
    }

    #[test]
    fn projects_ignore_the_selected_project() {
        let project = Project {
            id: uuid(OTHER_PROJECT),
            name: "shop".to_string(),
            description: None,
            service_summary: Default::default(),
        };
        assert_eq!(
            pick(vec![project], Some("shop"), selected()).unwrap().name,
            "shop"
        );
    }

    #[test]
    fn an_unknown_name_is_not_found() {
        let error = pick(vec![database(1, "main", None)], Some("web"), None).unwrap_err();
        assert_eq!(error.to_string(), "database 'web' not found");
    }
}
