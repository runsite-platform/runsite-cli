//! Maps API status strings to how they look. Unknown strings never panic:
//! they get a neutral `?` glyph.

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
    pub transitional: bool,
}

const fn look(glyph: Glyph, tone: Tone) -> StatusLook {
    StatusLook {
        glyph,
        tone,
        transitional: false,
    }
}

const BUSY: StatusLook = StatusLook {
    glyph: Glyph::Busy,
    tone: Tone::Neutral,
    transitional: true,
};

const UNKNOWN: StatusLook = look(Glyph::Unknown, Tone::Neutral);

pub fn service_look(status: &str) -> StatusLook {
    match status {
        "running" => look(Glyph::Running, Tone::Good),
        "stopped" => look(Glyph::Stopped, Tone::Muted),
        "sleeping" => look(Glyph::Sleeping, Tone::Muted),
        "deploying" | "starting" | "stopping" | "restarting" => BUSY,
        "failed" => look(Glyph::Failed, Tone::Bad),
        "blocked" | "pending_deletion" => look(Glyph::Blocked, Tone::Bad),
        _ => UNKNOWN,
    }
}

pub fn database_look(status: &str) -> StatusLook {
    match status {
        "running" => look(Glyph::Running, Tone::Good),
        "stopped" => look(Glyph::Stopped, Tone::Muted),
        "failed" => look(Glyph::Failed, Tone::Bad),
        "creating" | "starting" | "stopping" => BUSY,
        "suspended" | "marked_for_deletion" => look(Glyph::Blocked, Tone::Bad),
        // `/databases/{id}` returns 404 for these, so the row is greyed out.
        "pending_deletion" => look(Glyph::Blocked, Tone::Muted),
        _ => UNKNOWN,
    }
}

pub fn deployment_look(status: &str) -> StatusLook {
    match status {
        "running" => look(Glyph::Running, Tone::Good),
        "failed" => look(Glyph::Failed, Tone::Bad),
        "cancelled" => look(Glyph::Stopped, Tone::Muted),
        "pending" | "cloning" | "building" | "deploying" | "rolling_back" => BUSY,
        _ => UNKNOWN,
    }
}

pub fn deployment_in_progress(status: &str) -> bool {
    deployment_look(status).transitional
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_documented_service_status_has_a_look() {
        assert_eq!(service_look("running").glyph, Glyph::Running);
        assert_eq!(service_look("stopped").glyph, Glyph::Stopped);
        assert_eq!(service_look("sleeping").glyph, Glyph::Sleeping);
        assert_eq!(service_look("failed").tone, Tone::Bad);
        assert_eq!(service_look("blocked").glyph, Glyph::Blocked);
        assert_eq!(service_look("pending_deletion").glyph, Glyph::Blocked);
        for status in ["deploying", "starting", "stopping", "restarting"] {
            assert!(service_look(status).transitional, "{status}");
        }
    }

    #[test]
    fn every_documented_database_status_has_a_look() {
        assert_eq!(database_look("running").tone, Tone::Good);
        assert_eq!(database_look("stopped").glyph, Glyph::Stopped);
        assert_eq!(database_look("failed").glyph, Glyph::Failed);
        assert_eq!(database_look("suspended").glyph, Glyph::Blocked);
        assert_eq!(database_look("marked_for_deletion").glyph, Glyph::Blocked);
        assert_eq!(database_look("pending_deletion").tone, Tone::Muted);
        for status in ["creating", "starting", "stopping"] {
            assert!(database_look(status).transitional, "{status}");
        }
    }

    #[test]
    fn every_documented_deployment_status_has_a_look() {
        for status in [
            "pending",
            "cloning",
            "building",
            "deploying",
            "rolling_back",
        ] {
            assert!(deployment_in_progress(status), "{status}");
        }
        for status in ["running", "failed", "cancelled"] {
            assert!(!deployment_in_progress(status), "{status}");
        }
        assert_eq!(deployment_look("running").tone, Tone::Good);
        assert_eq!(deployment_look("failed").glyph, Glyph::Failed);
        assert_eq!(deployment_look("cancelled").glyph, Glyph::Stopped);
    }

    #[test]
    fn an_unknown_status_is_neutral_and_settled() {
        assert_eq!(service_look("hibernating"), UNKNOWN);
        assert_eq!(database_look(""), UNKNOWN);
        assert_eq!(deployment_look("queued"), UNKNOWN);
        assert!(!service_look("hibernating").transitional);
    }
}
