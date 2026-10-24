//! How a status looks on screen. What a status means lives in `api::Status`;
//! this only maps its condition to a glyph and a tone.

use crate::api::{Condition, Status};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Good,
    Muted,
    Bad,
    Neutral,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Glyph {
    Running,
    Stopped,
    Sleeping,
    Failed,
    Blocked,
    Busy,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StatusLook {
    pub glyph: Glyph,
    pub tone: Tone,
}

pub fn look(status: &impl Status) -> StatusLook {
    let (glyph, tone) = match status.condition() {
        Condition::Running => (Glyph::Running, Tone::Good),
        Condition::Stopped => (Glyph::Stopped, Tone::Muted),
        Condition::Sleeping => (Glyph::Sleeping, Tone::Muted),
        Condition::Busy => (Glyph::Busy, Tone::Neutral),
        Condition::Failed => (Glyph::Failed, Tone::Bad),
        Condition::Blocked => (Glyph::Blocked, Tone::Bad),
        Condition::Deleting => (Glyph::Blocked, Tone::Muted),
        Condition::Unknown => (Glyph::Unknown, Tone::Neutral),
    };
    StatusLook { glyph, tone }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{DatabaseStatus, DeploymentStatus, ServiceStatus};

    #[test]
    fn statuses_look_by_their_condition() {
        assert_eq!(look(&ServiceStatus::Running).tone, Tone::Good);
        assert_eq!(look(&ServiceStatus::Sleeping).glyph, Glyph::Sleeping);
        assert_eq!(look(&ServiceStatus::Blocked).tone, Tone::Bad);
        assert_eq!(look(&DatabaseStatus::Suspended).glyph, Glyph::Blocked);
        assert_eq!(look(&DeploymentStatus::Cancelled).glyph, Glyph::Stopped);
        assert_eq!(look(&DeploymentStatus::Building).glyph, Glyph::Busy);
    }

    #[test]
    fn deletion_is_greyed_out_not_an_error() {
        let deleting = StatusLook {
            glyph: Glyph::Blocked,
            tone: Tone::Muted,
        };
        assert_eq!(look(&ServiceStatus::PendingDeletion), deleting);
        assert_eq!(look(&DatabaseStatus::PendingDeletion), deleting);
    }

    #[test]
    fn an_unknown_status_is_neutral() {
        let unknown = look(&ServiceStatus::from("hibernating"));
        assert_eq!(
            unknown,
            StatusLook {
                glyph: Glyph::Unknown,
                tone: Tone::Neutral
            }
        );
    }
}
