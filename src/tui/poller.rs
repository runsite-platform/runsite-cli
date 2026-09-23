//! Decides which requests are due. Pure: the caller passes the current time.

use super::action::Request;
use chrono::{DateTime, TimeDelta, Utc};
use std::collections::BTreeMap;

const MAX_BACKOFF_SECONDS: i64 = 30;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cadence {
    Every(TimeDelta),
    /// Fetched once while active; retried only after a back-off failure.
    Once,
}

impl Cadence {
    pub fn seconds(seconds: i64) -> Self {
        Cadence::Every(TimeDelta::seconds(seconds))
    }
}

#[derive(Debug)]
struct Schedule {
    cadence: Cadence,
    next_due: Option<DateTime<Utc>>,
    in_flight: bool,
    failures: u32,
}

#[derive(Debug, Default)]
pub struct Poller {
    schedules: BTreeMap<Request, Schedule>,
}

/// 2, 4, 8, 16, then 30 seconds for every later failure.
pub fn backoff_delay(failures: u32) -> TimeDelta {
    let seconds = 2i64
        .saturating_pow(failures.max(1))
        .min(MAX_BACKOFF_SECONDS);
    TimeDelta::seconds(seconds)
}

impl Poller {
    /// Make exactly `desired` active. New requests are due immediately; a
    /// shorter interval pulls the next fetch in.
    pub fn sync(&mut self, desired: &[(Request, Cadence)], now: DateTime<Utc>) {
        self.schedules
            .retain(|request, _| desired.iter().any(|(wanted, _)| wanted == request));

        for (request, cadence) in desired {
            match self.schedules.get_mut(request) {
                Some(schedule) => {
                    if schedule.cadence == *cadence {
                        continue;
                    }
                    schedule.cadence = *cadence;
                    let Cadence::Every(interval) = cadence else {
                        continue;
                    };
                    if schedule.failures > 0 || schedule.in_flight {
                        continue;
                    }
                    // A finished `Once` request has no next fetch; start one.
                    schedule.next_due = Some(match schedule.next_due {
                        Some(due) => due.min(now + *interval),
                        None => now + *interval,
                    });
                }
                None => {
                    self.schedules.insert(
                        *request,
                        Schedule {
                            cadence: *cadence,
                            next_due: Some(now),
                            in_flight: false,
                            failures: 0,
                        },
                    );
                }
            }
        }
    }

    pub fn take_due(&mut self, now: DateTime<Utc>) -> Vec<Request> {
        let mut due = Vec::new();
        for (request, schedule) in self.schedules.iter_mut() {
            if schedule.in_flight || schedule.next_due.is_none_or(|next| next > now) {
                continue;
            }
            schedule.in_flight = true;
            schedule.next_due = None;
            due.push(*request);
        }
        due
    }

    pub fn record_success(&mut self, request: Request, now: DateTime<Utc>) {
        if let Some(schedule) = self.schedules.get_mut(&request) {
            schedule.in_flight = false;
            schedule.failures = 0;
            schedule.next_due = next_regular_fetch(schedule.cadence, now);
        }
    }

    /// `back_off` is true for network errors, 5xx and 429. A `Once` request
    /// always backs off: it has no next regular fetch to fall back on.
    pub fn record_failure(&mut self, request: Request, now: DateTime<Utc>, back_off: bool) {
        if let Some(schedule) = self.schedules.get_mut(&request) {
            schedule.in_flight = false;
            if back_off || schedule.cadence == Cadence::Once {
                schedule.failures += 1;
                schedule.next_due = Some(now + backoff_delay(schedule.failures));
            } else {
                schedule.failures = 0;
                schedule.next_due = next_regular_fetch(schedule.cadence, now);
            }
        }
    }

    /// `r`: fetch every periodic request now.
    pub fn refresh_all(&mut self, now: DateTime<Utc>) {
        for schedule in self.schedules.values_mut() {
            if !schedule.in_flight && matches!(schedule.cadence, Cadence::Every(_)) {
                schedule.next_due = Some(now);
            }
        }
    }

    pub fn is_active(&self, request: &Request) -> bool {
        self.schedules.contains_key(request)
    }

    pub fn backing_off(&self) -> bool {
        self.schedules
            .values()
            .any(|schedule| schedule.failures > 0)
    }

    pub fn clear(&mut self) {
        self.schedules.clear();
    }
}

fn next_regular_fetch(cadence: Cadence, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
    match cadence {
        Cadence::Every(interval) => Some(now + interval),
        Cadence::Once => None,
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
    fn a_new_request_is_due_at_once_and_not_twice_while_in_flight() {
        let mut poller = Poller::default();
        poller.sync(&[(Request::Projects, Cadence::seconds(30))], at(0));

        assert_eq!(poller.take_due(at(0)), vec![Request::Projects]);
        assert!(poller.take_due(at(100)).is_empty());
    }

    #[test]
    fn a_success_schedules_the_next_fetch_after_the_interval() {
        let mut poller = Poller::default();
        poller.sync(&[(Request::Projects, Cadence::seconds(30))], at(0));
        poller.take_due(at(0));
        poller.record_success(Request::Projects, at(1));

        assert!(poller.take_due(at(30)).is_empty());
        assert_eq!(poller.take_due(at(31)), vec![Request::Projects]);
    }

    #[test]
    fn failures_back_off_up_to_thirty_seconds_and_reset_on_success() {
        let delays: Vec<i64> = (1..=6)
            .map(|failures| backoff_delay(failures).num_seconds())
            .collect();
        assert_eq!(delays, vec![2, 4, 8, 16, 30, 30]);

        let mut poller = Poller::default();
        poller.sync(&[(Request::Projects, Cadence::seconds(30))], at(0));
        poller.take_due(at(0));
        poller.record_failure(Request::Projects, at(0), true);
        assert!(poller.backing_off());
        assert!(poller.take_due(at(1)).is_empty());
        assert_eq!(poller.take_due(at(2)), vec![Request::Projects]);

        poller.record_success(Request::Projects, at(2));
        assert!(!poller.backing_off());
    }

    #[test]
    fn a_failure_without_back_off_keeps_the_regular_interval() {
        let mut poller = Poller::default();
        poller.sync(&[(Request::Projects, Cadence::seconds(30))], at(0));
        poller.take_due(at(0));
        poller.record_failure(Request::Projects, at(0), false);

        assert!(!poller.backing_off());
        assert!(poller.take_due(at(29)).is_empty());
        assert_eq!(poller.take_due(at(30)), vec![Request::Projects]);
    }

    #[test]
    fn a_once_request_is_not_repeated_after_success() {
        let mut poller = Poller::default();
        poller.sync(&[(Request::KeyScope, Cadence::Once)], at(0));
        poller.take_due(at(0));
        poller.record_success(Request::KeyScope, at(0));
        poller.refresh_all(at(5));

        assert!(poller.take_due(at(1000)).is_empty());
    }

    #[test]
    fn a_failed_once_request_is_retried() {
        let mut poller = Poller::default();
        poller.sync(&[(Request::KeyScope, Cadence::Once)], at(0));
        poller.take_due(at(0));
        poller.record_failure(Request::KeyScope, at(0), false);

        assert_eq!(poller.take_due(at(2)), vec![Request::KeyScope]);
    }

    #[test]
    fn a_shorter_interval_pulls_the_next_fetch_in() {
        let mut poller = Poller::default();
        poller.sync(&[(Request::Projects, Cadence::seconds(30))], at(0));
        poller.take_due(at(0));
        poller.record_success(Request::Projects, at(0));

        poller.sync(&[(Request::Projects, Cadence::seconds(1))], at(5));
        assert_eq!(poller.take_due(at(6)), vec![Request::Projects]);
    }

    #[test]
    fn a_finished_once_request_resumes_when_it_becomes_periodic() {
        let mut poller = Poller::default();
        poller.sync(&[(Request::KeyScope, Cadence::Once)], at(0));
        poller.take_due(at(0));
        poller.record_success(Request::KeyScope, at(0));

        poller.sync(&[(Request::KeyScope, Cadence::seconds(3))], at(10));
        assert!(poller.take_due(at(12)).is_empty());
        assert_eq!(poller.take_due(at(13)), vec![Request::KeyScope]);
    }

    #[test]
    fn requests_no_longer_desired_are_dropped() {
        let mut poller = Poller::default();
        poller.sync(
            &[
                (Request::Projects, Cadence::seconds(30)),
                (Request::UnassignedServices, Cadence::seconds(30)),
            ],
            at(0),
        );
        poller.sync(&[(Request::Projects, Cadence::seconds(30))], at(1));

        assert!(poller.is_active(&Request::Projects));
        assert!(!poller.is_active(&Request::UnassignedServices));
    }

    #[test]
    fn refresh_makes_every_idle_periodic_request_due() {
        let mut poller = Poller::default();
        poller.sync(&[(Request::Projects, Cadence::seconds(30))], at(0));
        poller.take_due(at(0));
        poller.record_success(Request::Projects, at(0));

        poller.refresh_all(at(3));
        assert_eq!(poller.take_due(at(3)), vec![Request::Projects]);
    }
}
