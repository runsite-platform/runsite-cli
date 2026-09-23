//! Which remote actions are allowed right now, and why not when they are not.

use super::action::Mutation;
use super::status::{deployment_in_progress, service_look};
use crate::api::{DeploymentInfo, ServiceInfo};
use uuid::Uuid;

pub const READ_ONLY_REASON: &str = "needs a write key";
pub const IN_PROGRESS_REASON: &str = "a deployment is in progress";
pub const SCOPE_UNKNOWN_REASON: &str = "checking the key's permissions";

/// A disabled action: what the user pressed and why it cannot run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unavailable {
    pub action: &'static str,
    pub reason: String,
}

impl Unavailable {
    fn new(action: &'static str, reason: impl Into<String>) -> Self {
        Self {
            action,
            reason: reason.into(),
        }
    }

    pub fn message(&self) -> String {
        format!("{} is unavailable: {}", self.action, self.reason)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ServiceKey {
    Deploy,
    Restart,
    StartStop,
}

impl ServiceKey {
    pub fn label(self) -> &'static str {
        match self {
            ServiceKey::Deploy => "Deploy",
            ServiceKey::Restart => "Restart",
            ServiceKey::StartStop => "Start/stop",
        }
    }
}

/// Why nothing can be done with a service in this status, if that is the case.
fn frozen_reason(status: &str) -> Option<String> {
    match status {
        "blocked" => Some("blocked by policy".to_string()),
        "pending_deletion" => Some("being deleted".to_string()),
        _ if service_look(status).transitional => Some(format!("the service is {status}")),
        "running" | "stopped" | "sleeping" | "failed" => None,
        other => Some(format!("unknown status `{other}`")),
    }
}

fn check_scope(action: &'static str, key_scope: Option<&str>) -> Result<(), Unavailable> {
    match key_scope {
        Some("read") => Err(Unavailable::new(action, READ_ONLY_REASON)),
        // Until `users/me/api-key` answers, a read key must not look writable.
        None => Err(Unavailable::new(action, SCOPE_UNKNOWN_REASON)),
        Some(_) => Ok(()),
    }
}

/// `D`, `R` or `S` on a service. `deployment_running` is true while any
/// deployment of it is pending, building, deploying or rolling back.
pub fn service_mutation(
    key: ServiceKey,
    service: &ServiceInfo,
    deployment_running: bool,
    key_scope: Option<&str>,
) -> Result<Mutation, Unavailable> {
    let action = key.label();
    check_scope(action, key_scope)?;
    if let Some(reason) = frozen_reason(&service.status) {
        return Err(Unavailable::new(action, reason));
    }
    let running = service.status == "running";
    let service_id = service.id;
    match key {
        ServiceKey::Deploy if deployment_running => {
            Err(Unavailable::new(action, IN_PROGRESS_REASON))
        }
        ServiceKey::Deploy => Ok(Mutation::Deploy { service_id }),
        ServiceKey::Restart if deployment_running => {
            Err(Unavailable::new(action, IN_PROGRESS_REASON))
        }
        ServiceKey::Restart if running => Ok(Mutation::Restart { service_id }),
        ServiceKey::Restart => Err(Unavailable::new(
            action,
            format!("the service is {}", service.status),
        )),
        ServiceKey::StartStop if running => Ok(Mutation::StopService { service_id }),
        ServiceKey::StartStop => Ok(Mutation::StartService { service_id }),
    }
}

/// `B` on a deployment of `service`.
pub fn rollback_mutation(
    service: &ServiceInfo,
    target: &DeploymentInfo,
    deployments: &[DeploymentInfo],
    latest: Option<&DeploymentInfo>,
    key_scope: Option<&str>,
) -> Result<Mutation, Unavailable> {
    let action = "Rollback";
    check_scope(action, key_scope)?;
    // Transitional statuses do not block a rollback; unknown ones do.
    if !service_look(&service.status).transitional {
        if let Some(reason) = frozen_reason(&service.status) {
            return Err(Unavailable::new(action, reason));
        }
    }
    let any_in_progress = deployments
        .iter()
        .chain(latest)
        .any(|deployment| deployment_in_progress(&deployment.status));
    if any_in_progress {
        return Err(Unavailable::new(action, IN_PROGRESS_REASON));
    }
    if target.is_live {
        return Err(Unavailable::new(action, "this deployment is already live"));
    }
    if target.status != "running" {
        return Err(Unavailable::new(
            action,
            format!(
                "only a ready deployment can be restored (this one is {})",
                target.status
            ),
        ));
    }
    Ok(Mutation::Rollback {
        service_id: service.id,
        deployment_id: target.id,
    })
}

/// `S` on a database.
pub fn database_mutation(
    database_id: Uuid,
    status: &str,
    key_scope: Option<&str>,
) -> Result<Mutation, Unavailable> {
    let action = "Start/stop";
    check_scope(action, key_scope)?;
    match status {
        "running" => Ok(Mutation::StopDatabase { database_id }),
        "stopped" | "failed" => Ok(Mutation::StartDatabase { database_id }),
        "creating" | "starting" | "stopping" => Err(Unavailable::new(
            action,
            format!("the database is {status}"),
        )),
        "suspended" => Err(Unavailable::new(action, "the database is suspended")),
        "marked_for_deletion" | "pending_deletion" => {
            Err(Unavailable::new(action, "being deleted"))
        }
        other => Err(Unavailable::new(
            action,
            format!("unknown status `{other}`"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::fixtures::{
        deployment, service, API_SERVICE, DEPLOYMENT_LIVE, DEPLOYMENT_OLD, POSTGRES,
    };

    const WRITE: Option<&str> = Some("write");

    fn api(status: &str) -> ServiceInfo {
        service(API_SERVICE, "api", status, None)
    }

    #[test]
    fn a_running_service_can_be_deployed_restarted_and_stopped() {
        let running = api("running");
        assert_eq!(
            service_mutation(ServiceKey::Deploy, &running, false, Some("write")),
            Ok(Mutation::Deploy {
                service_id: API_SERVICE
            })
        );
        assert_eq!(
            service_mutation(ServiceKey::Restart, &running, false, WRITE),
            Ok(Mutation::Restart {
                service_id: API_SERVICE
            })
        );
        assert_eq!(
            service_mutation(ServiceKey::StartStop, &running, false, WRITE),
            Ok(Mutation::StopService {
                service_id: API_SERVICE
            })
        );
    }

    #[test]
    fn a_stopped_sleeping_or_failed_service_can_be_started_but_not_restarted() {
        for status in ["stopped", "sleeping", "failed"] {
            let idle = api(status);
            assert_eq!(
                service_mutation(ServiceKey::StartStop, &idle, false, WRITE),
                Ok(Mutation::StartService {
                    service_id: API_SERVICE
                }),
                "{status}"
            );
            assert!(service_mutation(ServiceKey::Deploy, &idle, false, WRITE).is_ok());
            let refused = service_mutation(ServiceKey::Restart, &idle, false, WRITE).unwrap_err();
            assert_eq!(refused.reason, format!("the service is {status}"));
        }
    }

    #[test]
    fn transitional_blocked_and_unknown_services_allow_nothing() {
        let cases = [
            ("restarting", "the service is restarting"),
            ("blocked", "blocked by policy"),
            ("pending_deletion", "being deleted"),
            ("hibernating", "unknown status `hibernating`"),
        ];
        for (status, reason) in cases {
            for key in [
                ServiceKey::Deploy,
                ServiceKey::Restart,
                ServiceKey::StartStop,
            ] {
                let refused = service_mutation(key, &api(status), false, WRITE).unwrap_err();
                assert_eq!(refused.reason, reason, "{status} {key:?}");
            }
        }
    }

    #[test]
    fn a_deployment_in_progress_blocks_deploy_and_restart_only() {
        let running = api("running");
        for key in [ServiceKey::Deploy, ServiceKey::Restart] {
            assert_eq!(
                service_mutation(key, &running, true, WRITE)
                    .unwrap_err()
                    .reason,
                IN_PROGRESS_REASON
            );
        }
        assert!(service_mutation(ServiceKey::StartStop, &running, true, WRITE).is_ok());
    }

    #[test]
    fn a_read_key_disables_every_action() {
        let running = api("running");
        let refused =
            service_mutation(ServiceKey::Deploy, &running, false, Some("read")).unwrap_err();
        assert_eq!(
            refused.message(),
            "Deploy is unavailable: needs a write key"
        );
        let ready = deployment(DEPLOYMENT_OLD, "running", false, "7b3e0f2a");
        assert!(rollback_mutation(&running, &ready, &[], None, Some("read")).is_err());
        assert!(database_mutation(POSTGRES, "running", Some("read")).is_err());
    }

    #[test]
    fn nothing_is_allowed_before_the_key_scope_is_known() {
        let refused =
            service_mutation(ServiceKey::Deploy, &api("running"), false, None).unwrap_err();
        assert_eq!(refused.reason, SCOPE_UNKNOWN_REASON);
        assert!(database_mutation(POSTGRES, "running", None).is_err());
    }

    #[test]
    fn rollback_is_refused_for_an_unknown_service_status() {
        let ready = deployment(DEPLOYMENT_OLD, "running", false, "7b3e0f2a");
        assert!(rollback_mutation(&api("hibernating"), &ready, &[], None, WRITE).is_err());
        assert!(rollback_mutation(&api("restarting"), &ready, &[], None, WRITE).is_ok());
    }

    #[test]
    fn rollback_needs_a_ready_deployment_that_is_not_live() {
        let running = api("running");
        let live = deployment(DEPLOYMENT_LIVE, "running", true, "3f2a9c1d");
        let ready = deployment(DEPLOYMENT_OLD, "running", false, "7b3e0f2a");
        let failed = deployment(DEPLOYMENT_OLD, "failed", false, "7b3e0f2a");
        let history = vec![live.clone(), ready.clone()];

        assert_eq!(
            rollback_mutation(&running, &ready, &history, None, WRITE),
            Ok(Mutation::Rollback {
                service_id: API_SERVICE,
                deployment_id: DEPLOYMENT_OLD
            })
        );
        assert!(rollback_mutation(&running, &live, &history, None, WRITE).is_err());
        assert!(rollback_mutation(&running, &failed, &history, None, WRITE).is_err());
    }

    #[test]
    fn rollback_waits_for_deployments_in_progress_and_frozen_services() {
        let ready = deployment(DEPLOYMENT_OLD, "running", false, "7b3e0f2a");
        let building = deployment(DEPLOYMENT_LIVE, "building", false, "4b1c2d3e");
        assert_eq!(
            rollback_mutation(&api("running"), &ready, &[], Some(&building), WRITE)
                .unwrap_err()
                .reason,
            IN_PROGRESS_REASON
        );
        assert!(rollback_mutation(&api("blocked"), &ready, &[], None, WRITE).is_err());
        assert!(
            rollback_mutation(&api("stopped"), &ready, &[], None, WRITE).is_ok(),
            "a stopped service can still be rolled back"
        );
    }

    #[test]
    fn databases_toggle_between_running_and_stopped() {
        assert_eq!(
            database_mutation(POSTGRES, "running", WRITE),
            Ok(Mutation::StopDatabase {
                database_id: POSTGRES
            })
        );
        for status in ["stopped", "failed"] {
            assert_eq!(
                database_mutation(POSTGRES, status, WRITE),
                Ok(Mutation::StartDatabase {
                    database_id: POSTGRES
                })
            );
        }
        for status in [
            "creating",
            "starting",
            "stopping",
            "suspended",
            "marked_for_deletion",
            "odd",
        ] {
            assert!(
                database_mutation(POSTGRES, status, WRITE).is_err(),
                "{status}"
            );
        }
    }
}
