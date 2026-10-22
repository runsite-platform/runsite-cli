//! The gate in front of every remote action: what an action key would do on
//! the current screen, whether it is allowed, the confirm modal, and the rule
//! that only one action runs at a time. Only `y` in the modal sends a request.

use super::action::{Effect, Mutation};
use super::actions::{
    database_mutation, rollback_mutation, service_mutation, ActionKey, Unavailable,
};
use super::app::{App, Overlay, PendingAction, Screen};
use super::dashboard::{Pane, Resource};
use super::detail::{short_ref, DetailTab};
use super::status::Tone;
use crate::api::ServiceInfo;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Confirm {
    pub mutation: Mutation,
    /// Rendered as `{verb} **{target}**{suffix}`, e.g. "Restart **api**?".
    pub target: String,
    pub suffix: String,
}

impl Confirm {
    pub fn verb(&self) -> &'static str {
        self.mutation.verb()
    }
}

/// An action key as the status bar shows it; disabled ones are greyed out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActionHint {
    pub key: &'static str,
    pub hint: &'static str,
    pub enabled: bool,
}

fn service_confirm(mutation: Mutation, service: &ServiceInfo) -> Confirm {
    let suffix = match mutation {
        Mutation::Deploy { .. } => match (&service.image_ref, &service.github_branch) {
            (Some(image), _) if service.source_type.as_deref() == Some("image") => {
                format!(" from image {image}?")
            }
            (_, Some(branch)) => format!(" from branch {branch}?"),
            _ => "?".to_string(),
        },
        _ => "?".to_string(),
    };
    Confirm {
        mutation,
        target: service.name.clone(),
        suffix,
    }
}

fn plain_confirm(mutation: Mutation, target: &str) -> Confirm {
    Confirm {
        mutation,
        target: target.to_string(),
        suffix: "?".to_string(),
    }
}

impl App {
    /// What an action key would do on the current screen. `None` when the
    /// key means nothing here, `Err` when it is disabled.
    pub fn resolve_action(&self, key: ActionKey) -> Option<Result<Confirm, Unavailable>> {
        let resolved = self.resolve_action_on_screen(key)?;
        if let (Ok(confirm), Some(_)) = (&resolved, self.pending_action) {
            return Some(Err(Unavailable {
                action: confirm.verb(),
                reason: "waiting for the previous action".to_string(),
            }));
        }
        Some(resolved)
    }

    /// `None` when `key` means nothing on the current screen.
    pub fn action_hint(&self, key: ActionKey) -> Option<ActionHint> {
        let resolved = self.resolve_action(key)?;
        let hint = match &resolved {
            Ok(confirm) => match confirm.mutation {
                Mutation::StartService { .. } | Mutation::StartDatabase { .. } => "start",
                Mutation::StopService { .. } | Mutation::StopDatabase { .. } => "stop",
                _ => key.hint(),
            },
            Err(_) => key.hint(),
        };
        Some(ActionHint {
            key: key.key(),
            hint,
            enabled: resolved.is_ok(),
        })
    }

    fn resolve_action_on_screen(&self, key: ActionKey) -> Option<Result<Confirm, Unavailable>> {
        let scope = self.session.key_scope.as_deref();
        match &self.screen {
            // Only the focused pane's selection is a target: in the one-pane
            // layout the resources list may not even be visible.
            Screen::Dashboard if self.dashboard.focus != Pane::Resources => None,
            Screen::Dashboard => match self.dashboard.selected_resource()? {
                Resource::Service(service) => {
                    let deployments = self.dashboard.deployment_activity(service.id);
                    Some(
                        service_mutation(key, service, deployments, scope)?
                            .map(|mutation| service_confirm(mutation, service)),
                    )
                }
                Resource::Postgres(database) if key == ActionKey::StartStop => Some(
                    database_mutation(database.id, &database.status, scope)
                        .map(|mutation| plain_confirm(mutation, &database.name)),
                ),
                Resource::Redis(database) if key == ActionKey::StartStop => Some(
                    database_mutation(database.id, &database.status, scope)
                        .map(|mutation| plain_confirm(mutation, &database.name)),
                ),
                _ => None,
            },
            Screen::ServiceDetail(detail) => {
                let service = detail.service.as_ref()?;
                if key != ActionKey::Rollback {
                    return Some(
                        service_mutation(key, service, detail.deployment_activity(), scope)?
                            .map(|mutation| service_confirm(mutation, service)),
                    );
                }
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
                Some(mutation.map(|mutation| Confirm {
                    mutation,
                    target: service.name.clone(),
                    suffix: format!(" to {}?", short_ref(target)),
                }))
            }
            Screen::DatabaseDetail(database) if key == ActionKey::StartStop => {
                let detail = database.detail.as_ref()?;
                Some(
                    database_mutation(database.database_id, &detail.status, scope)
                        .map(|mutation| plain_confirm(mutation, &database.name)),
                )
            }
            _ => None,
        }
    }

    /// Open the confirm modal for `key`, or say why it is unavailable. Does
    /// nothing when the key means nothing on this screen.
    pub(super) fn request_action(&mut self, key: ActionKey) {
        match self.resolve_action(key) {
            Some(Ok(confirm)) => self.overlay = Some(Overlay::Confirm(confirm)),
            Some(Err(unavailable)) => self.show_toast(unavailable.message(), Tone::Bad),
            None => {}
        }
    }

    /// `y` in the confirm modal.
    ///
    /// Polls keep running under the modal, so the action is checked again here:
    /// it is sent only if it is still allowed and still the same request.
    pub(super) fn confirm_action(&mut self) -> Vec<Effect> {
        let Some(Overlay::Confirm(confirm)) = &self.overlay else {
            return Vec::new();
        };
        let mutation = confirm.mutation;
        self.overlay = None;
        match self.resolve_action(mutation.action_key()) {
            Some(Ok(current)) if current.mutation == mutation => {
                self.pending_action = Some(PendingAction::InFlight);
                vec![Effect::Mutate {
                    generation: self.generation,
                    mutation,
                }]
            }
            Some(Err(unavailable)) => {
                self.show_toast(unavailable.message(), Tone::Bad);
                Vec::new()
            }
            _ => {
                self.show_toast("The status changed, nothing was sent", Tone::Bad);
                Vec::new()
            }
        }
    }
}
