//! Uppercase keys that change remote state: `D` deploy, `R` restart,
//! `S` start/stop, `B` rollback. Each one opens the confirm modal first.

use super::action::{Effect, Mutation};
use super::actions::{
    database_mutation, rollback_mutation, service_mutation, ServiceKey, Unavailable,
    CHECKING_DEPLOYMENT_REASON,
};
use super::app::{App, Overlay, Screen};
use super::dashboard::{Pane, Resource};
use super::detail::{short_ref, DetailTab};
use super::status::deployment_in_progress;
use super::status::Tone;
use crate::api::ServiceInfo;
use ratatui::crossterm::event::{KeyCode, KeyEvent};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Confirm {
    pub mutation: Mutation,
    /// Rendered as `{verb} **{target}**{suffix}`, e.g. "Restart **api**?".
    pub verb: &'static str,
    pub target: String,
    pub suffix: String,
}

fn service_confirm(mutation: Mutation, service: &ServiceInfo) -> Confirm {
    let (verb, suffix) = match mutation {
        Mutation::Deploy { .. } => {
            let source = match (&service.image_ref, &service.github_branch) {
                (Some(image), _) if service.source_type.as_deref() == Some("image") => {
                    format!(" from image {image}?")
                }
                (_, Some(branch)) => format!(" from branch {branch}?"),
                _ => "?".to_string(),
            };
            ("Deploy", source)
        }
        Mutation::Restart { .. } => ("Restart", "?".to_string()),
        Mutation::StartService { .. } => ("Start", "?".to_string()),
        _ => ("Stop", "?".to_string()),
    };
    Confirm {
        mutation,
        verb,
        target: service.name.clone(),
        suffix,
    }
}

fn service_key(key: char) -> Option<ServiceKey> {
    match key {
        'D' => Some(ServiceKey::Deploy),
        'R' => Some(ServiceKey::Restart),
        'S' => Some(ServiceKey::StartStop),
        _ => None,
    }
}

fn database_confirm(mutation: Mutation, name: &str) -> Confirm {
    let verb = match mutation {
        Mutation::StartDatabase { .. } => "Start",
        _ => "Stop",
    };
    Confirm {
        mutation,
        verb,
        target: name.to_string(),
        suffix: "?".to_string(),
    }
}

impl App {
    /// What an action key would do on the current screen. `None` when the
    /// key means nothing here, `Err` when it is disabled.
    pub fn resolve_action(&self, key: char) -> Option<Result<Confirm, Unavailable>> {
        let scope = self.session.key_scope.as_deref();
        match &self.screen {
            // Only the focused pane's selection is a target: in the one-pane
            // layout the resources list may not even be visible.
            Screen::Dashboard if self.dashboard.focus != Pane::Resources => None,
            Screen::Dashboard => match self.dashboard.selected_resource()? {
                Resource::Service(service) => {
                    let service_key = service_key(key)?;
                    let latest = self.dashboard.latest_deployments.get(&service.id);
                    let deployment_running = latest.is_some_and(|latest| {
                        latest
                            .as_ref()
                            .is_some_and(|deployment| deployment_in_progress(&deployment.status))
                    });
                    let mutation =
                        service_mutation(service_key, &service, deployment_running, scope);
                    // Scope and status reasons win; then wait for the first poll.
                    if mutation.is_ok() && latest.is_none() && service_key != ServiceKey::StartStop
                    {
                        return Some(Err(Unavailable {
                            action: service_key.label(),
                            reason: CHECKING_DEPLOYMENT_REASON.to_string(),
                        }));
                    }
                    Some(mutation.map(|mutation| service_confirm(mutation, &service)))
                }
                Resource::Postgres(database) if key == 'S' => Some(
                    database_mutation(database.id, &database.status, scope)
                        .map(|mutation| database_confirm(mutation, &database.name)),
                ),
                Resource::Redis(database) if key == 'S' => Some(
                    database_mutation(database.id, &database.status, scope)
                        .map(|mutation| database_confirm(mutation, &database.name)),
                ),
                _ => None,
            },
            Screen::ServiceDetail(detail) => {
                let service = detail.service.as_ref()?;
                if key == 'B' {
                    let target = match &detail.deployment_view {
                        Some(view) => view.detail.as_ref()?,
                        None if detail.tab == DetailTab::Deploys => detail.selected_deployment()?,
                        None => return None,
                    };
                    let mutation = rollback_mutation(
                        service,
                        target,
                        detail.deployments.as_deref().unwrap_or_default(),
                        detail.latest_deployment.as_ref(),
                        scope,
                    );
                    return Some(mutation.map(|mutation| Confirm {
                        mutation,
                        verb: "Roll back",
                        target: service.name.clone(),
                        suffix: format!(" to {}?", short_ref(target)),
                    }));
                }
                let service_key = service_key(key)?;
                let mutation =
                    service_mutation(service_key, service, detail.deployment_in_progress(), scope);
                // As on the Dashboard: a deploy may be running before the first poll says so.
                if mutation.is_ok()
                    && !detail.deployments_checked
                    && service_key != ServiceKey::StartStop
                {
                    return Some(Err(Unavailable {
                        action: service_key.label(),
                        reason: CHECKING_DEPLOYMENT_REASON.to_string(),
                    }));
                }
                Some(mutation.map(|mutation| service_confirm(mutation, service)))
            }
            Screen::DatabaseDetail(database) if key == 'S' => {
                let detail = database.detail.as_ref()?;
                Some(
                    database_mutation(database.database_id, &detail.status, scope)
                        .map(|mutation| database_confirm(mutation, &database.name)),
                )
            }
            _ => None,
        }
    }

    /// `None` when `key` is not an action key on this screen.
    pub(super) fn handle_action_key(&mut self, key: KeyEvent) -> Option<Vec<Effect>> {
        let KeyCode::Char(character @ ('D' | 'R' | 'S' | 'B')) = key.code else {
            return None;
        };
        match self.resolve_action(character)? {
            Ok(confirm) => self.overlay = Some(Overlay::Confirm(confirm)),
            Err(unavailable) => self.show_toast(unavailable.message(), Tone::Bad),
        }
        Some(Vec::new())
    }

    /// Only `y` sends the request; `n`, `Esc` and `Enter` cancel.
    pub(super) fn handle_confirm_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        let Some(Overlay::Confirm(confirm)) = &self.overlay else {
            return Vec::new();
        };
        match key.code {
            KeyCode::Char('y') => {
                let mutation = confirm.mutation;
                self.overlay = None;
                vec![Effect::Mutate {
                    generation: self.generation,
                    mutation,
                }]
            }
            KeyCode::Char('n') | KeyCode::Esc | KeyCode::Enter => {
                self.overlay = None;
                Vec::new()
            }
            _ => Vec::new(),
        }
    }
}
