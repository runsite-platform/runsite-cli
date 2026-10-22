//! Statuses the API reports for services, databases and deployments.
//!
//! Each status knows its [`Condition`], the one thing the CLI and the TUI
//! need to decide how it looks and what can be done. A status the API adds
//! later parses as `Unknown` instead of failing the whole response.

use std::fmt;

/// What a status means, whatever resource it belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Condition {
    Running,
    Stopped,
    Sleeping,
    /// Changing on its own: wait for it to settle.
    Busy,
    Failed,
    /// Stopped by policy or billing; the owner cannot bring it back.
    Blocked,
    Deleting,
    Unknown,
}

/// A status as the API spells it, and what it means.
pub trait Status: fmt::Display {
    fn condition(&self) -> Condition;

    fn is_transitional(&self) -> bool {
        self.condition() == Condition::Busy
    }
}

macro_rules! status_enum {
    ($(#[$meta:meta])* $name:ident { $($variant:ident => $text:literal => $condition:ident,)+ }) => {
        $(#[$meta])*
        #[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
        #[serde(from = "String", into = "String")]
        pub enum $name {
            $($variant,)+
            Unknown(String),
        }

        impl $name {
            pub fn as_str(&self) -> &str {
                match self {
                    $(Self::$variant => $text,)+
                    Self::Unknown(text) => text,
                }
            }
        }

        impl Status for $name {
            fn condition(&self) -> Condition {
                match self {
                    $(Self::$variant => Condition::$condition,)+
                    Self::Unknown(_) => Condition::Unknown,
                }
            }
        }

        impl From<&str> for $name {
            fn from(text: &str) -> Self {
                match text {
                    $($text => Self::$variant,)+
                    other => Self::Unknown(other.to_string()),
                }
            }
        }

        impl From<String> for $name {
            fn from(text: String) -> Self {
                Self::from(text.as_str())
            }
        }

        impl From<$name> for String {
            fn from(status: $name) -> Self {
                status.as_str().to_string()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.pad(self.as_str())
            }
        }
    };
}

status_enum! {
    /// `WebServiceStatus` in the API.
    ServiceStatus {
        Running => "running" => Running,
        Stopped => "stopped" => Stopped,
        Sleeping => "sleeping" => Sleeping,
        Deploying => "deploying" => Busy,
        Starting => "starting" => Busy,
        Stopping => "stopping" => Busy,
        Restarting => "restarting" => Busy,
        Failed => "failed" => Failed,
        Blocked => "blocked" => Blocked,
        PendingDeletion => "pending_deletion" => Deleting,
    }
}

status_enum! {
    /// `DatabaseStatus` and `RedisStatus` in the API.
    DatabaseStatus {
        Running => "running" => Running,
        Stopped => "stopped" => Stopped,
        Creating => "creating" => Busy,
        Starting => "starting" => Busy,
        Stopping => "stopping" => Busy,
        Failed => "failed" => Failed,
        Suspended => "suspended" => Blocked,
        PendingDeletion => "pending_deletion" => Deleting,
        MarkedForDeletion => "marked_for_deletion" => Deleting,
    }
}

status_enum! {
    /// `DeploymentStatus` in the API. `Running` means the deployment finished
    /// and can be restored; `is_live` tells which one serves traffic.
    DeploymentStatus {
        Pending => "pending" => Busy,
        Cloning => "cloning" => Busy,
        Building => "building" => Busy,
        Deploying => "deploying" => Busy,
        RollingBack => "rolling_back" => Busy,
        Running => "running" => Running,
        Failed => "failed" => Failed,
        Cancelled => "cancelled" => Stopped,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_status_keeps_its_api_spelling_both_ways() {
        let status: ServiceStatus = serde_json::from_str(r#""pending_deletion""#).unwrap();
        assert_eq!(status, ServiceStatus::PendingDeletion);
        assert_eq!(
            serde_json::to_string(&status).unwrap(),
            r#""pending_deletion""#
        );
        assert_eq!(status.to_string(), "pending_deletion");
        assert_eq!(format!("[{:<9}]", ServiceStatus::Running), "[running  ]");
    }

    #[test]
    fn an_unknown_status_survives_a_round_trip() {
        let status: DatabaseStatus = serde_json::from_str(r#""resizing""#).unwrap();
        assert_eq!(status, DatabaseStatus::Unknown("resizing".to_string()));
        assert_eq!(status.condition(), Condition::Unknown);
        assert!(!status.is_transitional());
        assert_eq!(serde_json::to_string(&status).unwrap(), r#""resizing""#);
    }

    #[test]
    fn transitional_statuses_are_busy() {
        for status in ["deploying", "starting", "stopping", "restarting"] {
            assert!(ServiceStatus::from(status).is_transitional(), "{status}");
        }
        for status in ["creating", "starting", "stopping"] {
            assert!(DatabaseStatus::from(status).is_transitional(), "{status}");
        }
        for status in [
            "pending",
            "cloning",
            "building",
            "deploying",
            "rolling_back",
        ] {
            assert!(DeploymentStatus::from(status).is_transitional(), "{status}");
        }
        for status in ["running", "failed", "cancelled"] {
            assert!(
                !DeploymentStatus::from(status).is_transitional(),
                "{status}"
            );
        }
    }

    #[test]
    fn deletion_reads_the_same_for_services_and_databases() {
        assert_eq!(
            ServiceStatus::PendingDeletion.condition(),
            Condition::Deleting
        );
        assert_eq!(
            DatabaseStatus::PendingDeletion.condition(),
            Condition::Deleting
        );
        assert_eq!(
            DatabaseStatus::MarkedForDeletion.condition(),
            Condition::Deleting
        );
    }
}
