//! The gate in front of every remote action: what an action key would do on
//! the current screen, whether it is allowed, the confirm modal, and the rule
//! that only one action runs at a time. Only `y` in the modal sends a request.

use super::action::{Effect, Mutation};
use super::actions::{ActionKey, Unavailable};
use super::app::{App, Overlay, Screen};
use super::status::Tone;
use crate::api::Service;
use chrono::{DateTime, TimeDelta, Utc};

/// How long actions stay blocked after one was accepted: two fast polls, so
/// the new status or deployment has been seen before the next action.
const SETTLE_SECONDS: i64 = 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pending {
    /// Sent in session `generation`, waiting for the result.
    InFlight {
        generation: u64,
    },
    Settling {
        until: DateTime<Utc>,
    },
}

/// Only one remote action at a time: from `y` until its result arrives, then
/// a few seconds more so the next action is judged on the new state.
#[derive(Debug, Default)]
pub struct ActionSlot {
    pending: Option<Pending>,
}

impl ActionSlot {
    pub fn is_busy(&self) -> bool {
        self.pending.is_some()
    }

    pub fn start(&mut self, generation: u64) {
        self.pending = Some(Pending::InFlight { generation });
    }

    /// The result of the action sent in session `generation`. A result from
    /// an earlier session leaves the current one alone.
    pub fn finish(&mut self, generation: u64, accepted: bool, now: DateTime<Utc>) {
        if self.pending != Some(Pending::InFlight { generation }) {
            return;
        }
        self.pending = accepted.then_some(Pending::Settling {
            until: now + TimeDelta::seconds(SETTLE_SECONDS),
        });
    }

    pub fn tick(&mut self, now: DateTime<Utc>) {
        if matches!(self.pending, Some(Pending::Settling { until }) if until <= now) {
            self.pending = None;
        }
    }

    /// A new session starts with nothing pending.
    pub fn clear(&mut self) {
        self.pending = None;
    }
}

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

pub(super) fn service_confirm(mutation: Mutation, service: &Service) -> Confirm {
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

pub(super) fn plain_confirm(mutation: Mutation, target: &str) -> Confirm {
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
        if let (Ok(confirm), true) = (&resolved, self.actions.is_busy()) {
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
            Screen::Dashboard => self.dashboard.resolve_action(key, scope),
            Screen::ServiceDetail(detail) => detail.resolve_action(key, scope),
            Screen::DatabaseDetail(database) => database.resolve_action(key, scope),
            Screen::Login(_) | Screen::Blocked { .. } => None,
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
                self.actions.start(self.generation);
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(second: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(1_790_000_000 + second, 0).unwrap()
    }

    #[test]
    fn an_accepted_action_settles_before_the_next_one() {
        let mut slot = ActionSlot::default();
        slot.start(0);
        slot.finish(0, true, at(0));

        slot.tick(at(SETTLE_SECONDS - 1));
        assert!(slot.is_busy());
        slot.tick(at(SETTLE_SECONDS));
        assert!(!slot.is_busy());
    }

    #[test]
    fn a_rejected_action_frees_the_slot_at_once() {
        let mut slot = ActionSlot::default();
        slot.start(0);
        slot.finish(0, false, at(0));
        assert!(!slot.is_busy());
    }

    #[test]
    fn a_result_from_an_earlier_session_leaves_the_current_action_alone() {
        let mut slot = ActionSlot::default();
        slot.start(0);
        slot.clear();
        slot.start(1);

        slot.finish(0, false, at(0));

        assert!(slot.is_busy());
    }
}
