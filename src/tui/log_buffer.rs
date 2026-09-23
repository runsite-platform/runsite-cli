//! Runtime log lines collected by polling `GET …/logs`.
//!
//! The backend turns `since` into a whole-second window and applies `tail`
//! before `since`, so every poll overlaps the previous one by a few seconds.
//! Lines in the overlap are de-duplicated by `(timestamp, text)` with a
//! multiset, which keeps genuinely repeated lines.

use chrono::{DateTime, TimeDelta, Utc};
use std::collections::{HashMap, VecDeque};

pub const RING_CAPACITY: usize = 5000;
pub const FIRST_TAIL: u32 = 500;
pub const FOLLOW_TAIL: u32 = 1000;
pub const OVERLAP_SECONDS: i64 = 5;
/// A Logs tab hidden longer than this starts over from a fresh tail.
pub const RESTART_AFTER_SECONDS: i64 = 30;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LogQuery {
    pub tail: u32,
    pub since: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LogLine {
    Entry {
        timestamp: Option<DateTime<Utc>>,
        text: String,
    },
    /// The response was full, so older lines in the window were lost.
    Gap,
    /// Polling restarted after the tab was hidden.
    Separator,
}

type LineKey = (DateTime<Utc>, String);

/// Split `2026-09-23T10:00:00.123Z message` into its timestamp and message.
pub fn parse_line(line: &str) -> Option<(DateTime<Utc>, &str)> {
    let (stamp, rest) = line.split_once(' ').unwrap_or((line, ""));
    let timestamp = DateTime::parse_from_rfc3339(stamp).ok()?;
    Some((timestamp.with_timezone(&Utc), rest))
}

#[derive(Debug)]
pub struct LogBuffer {
    lines: VecDeque<LogLine>,
    /// Newest timestamp seen. Unaffected by `clear_view` and ring eviction.
    cursor: Option<DateTime<Utc>>,
    recent: HashMap<LineKey, usize>,
    /// Shown instead of lines, e.g. "Service is not ready yet…".
    pub status_message: Option<String>,
    pub last_polled: Option<DateTime<Utc>>,
    pub follow: bool,
    pub wrap: bool,
    /// Lines scrolled up from the bottom while not following.
    pub scroll_from_bottom: usize,
    pub horizontal_offset: usize,
    pub search: String,
    pub editing_search: bool,
    pub current_match: Option<usize>,
    /// Query of the request in flight; other replies are stale.
    in_flight: Option<LogQuery>,
}

impl Default for LogBuffer {
    fn default() -> Self {
        Self {
            lines: VecDeque::new(),
            cursor: None,
            recent: HashMap::new(),
            status_message: None,
            last_polled: None,
            follow: true,
            wrap: true,
            scroll_from_bottom: 0,
            horizontal_offset: 0,
            search: String::new(),
            editing_search: false,
            current_match: None,
            in_flight: None,
        }
    }
}

impl LogBuffer {
    pub fn lines(&self) -> &VecDeque<LogLine> {
        &self.lines
    }

    pub fn next_query(&self) -> LogQuery {
        match self.cursor {
            None => LogQuery {
                tail: FIRST_TAIL,
                since: None,
            },
            Some(cursor) => LogQuery {
                tail: FOLLOW_TAIL,
                since: Some(cursor.timestamp() - OVERLAP_SECONDS),
            },
        }
    }

    /// The query for the next request, remembered until its reply arrives.
    pub fn start_fetch(&mut self) -> LogQuery {
        let query = self.next_query();
        self.in_flight = Some(query);
        query
    }

    /// True when `query` is the request this buffer is waiting for.
    pub fn finish_fetch(&mut self, query: LogQuery) -> bool {
        if self.in_flight == Some(query) {
            self.in_flight = None;
            true
        } else {
            false
        }
    }

    /// Start over from a fresh tail if the tab was hidden for too long.
    pub fn resume(&mut self, now: DateTime<Utc>) {
        let hidden_too_long = self
            .last_polled
            .is_some_and(|polled| now - polled > TimeDelta::seconds(RESTART_AFTER_SECONDS));
        if hidden_too_long {
            self.cursor = None;
            self.recent.clear();
            if !self.lines.is_empty() {
                self.push(LogLine::Separator);
            }
        }
    }

    pub fn ingest(&mut self, body: &str, query: LogQuery, now: DateTime<Utc>) {
        self.last_polled = Some(now);
        if body.trim().is_empty() {
            // Logs are readable again; only the buffer stays as it was.
            self.status_message = None;
            return;
        }

        let parsed: Vec<(Option<DateTime<Utc>>, &str)> = body
            .lines()
            .map(|line| match parse_line(line) {
                Some((timestamp, text)) => (Some(timestamp), text),
                None => (None, line),
            })
            .collect();
        if parsed.iter().all(|(timestamp, _)| timestamp.is_none()) {
            self.status_message = Some(body.trim().to_string());
            return;
        }
        self.status_message = None;

        let first_timestamp = parsed.iter().find_map(|(timestamp, _)| *timestamp);
        let response_was_full = parsed.len() as u32 >= query.tail;
        let since = query
            .since
            .and_then(|seconds| DateTime::from_timestamp(seconds, 0));
        if let (Some(since), Some(first), true) = (since, first_timestamp, response_was_full) {
            if first > since {
                self.push(LogLine::Gap);
            }
        }

        let mut already_seen = self.recent.clone();
        for (timestamp, text) in parsed {
            if let Some(timestamp) = timestamp {
                let key = (timestamp, text.to_string());
                if let Some(count) = already_seen.get_mut(&key).filter(|count| **count > 0) {
                    *count -= 1;
                    continue;
                }
                *self.recent.entry(key).or_insert(0) += 1;
                self.cursor = Some(
                    self.cursor
                        .map_or(timestamp, |cursor| cursor.max(timestamp)),
                );
            }
            self.push(LogLine::Entry {
                timestamp,
                text: text.to_string(),
            });
        }
        self.forget_old_keys();
    }

    fn forget_old_keys(&mut self) {
        let Some(cursor) = self.cursor else {
            return;
        };
        let horizon = cursor - TimeDelta::seconds(OVERLAP_SECONDS * 2);
        self.recent
            .retain(|(timestamp, _), _| *timestamp >= horizon);
    }

    fn push(&mut self, line: LogLine) {
        self.lines.push_back(line);
        if !self.follow {
            self.scroll_from_bottom += 1;
        }
        while self.lines.len() > RING_CAPACITY {
            self.lines.pop_front();
            self.current_match = self.current_match.and_then(|index| index.checked_sub(1));
        }
        self.scroll_from_bottom = self.scroll_from_bottom.min(self.lines.len());
    }

    /// `c`: empty the view. The cursor stays, so old lines do not come back.
    pub fn clear_view(&mut self) {
        self.lines.clear();
        self.scroll_from_bottom = 0;
        self.current_match = None;
    }

    pub fn scroll_up(&mut self, lines: usize) {
        self.follow = false;
        self.scroll_from_bottom = (self.scroll_from_bottom + lines).min(self.lines.len());
    }

    pub fn scroll_down(&mut self, lines: usize) {
        self.scroll_from_bottom = self.scroll_from_bottom.saturating_sub(lines);
    }

    pub fn jump_to_top(&mut self) {
        self.follow = false;
        self.scroll_from_bottom = self.lines.len();
    }

    /// `G` and `f`: back to the newest line, following new output.
    pub fn follow_tail(&mut self) {
        self.follow = true;
        self.scroll_from_bottom = 0;
    }

    fn matches(&self, index: usize) -> bool {
        let needle = self.search.to_lowercase();
        match &self.lines[index] {
            LogLine::Entry { text, .. } => {
                !needle.is_empty() && text.to_lowercase().contains(&needle)
            }
            _ => false,
        }
    }

    pub fn is_match(&self, index: usize) -> bool {
        self.matches(index)
    }

    /// Enter in the search box: jump to the newest match.
    pub fn apply_search(&mut self) {
        self.editing_search = false;
        self.current_match = (0..self.lines.len())
            .rev()
            .find(|index| self.matches(*index));
        self.reveal_match();
    }

    /// `n` moves to a newer match, `N` to an older one.
    pub fn step_match(&mut self, newer: bool) {
        let start = self.current_match.unwrap_or(self.lines.len());
        let found = if newer {
            (start + 1..self.lines.len()).find(|index| self.matches(*index))
        } else {
            (0..start).rev().find(|index| self.matches(*index))
        };
        if found.is_some() {
            self.current_match = found;
            self.reveal_match();
        }
    }

    fn reveal_match(&mut self) {
        if let Some(index) = self.current_match {
            self.follow = false;
            self.scroll_from_bottom = self.lines.len() - 1 - index;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    const BASE: i64 = 1_790_157_600; // 2026-09-23T10:00:00Z

    fn at(second: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(BASE + second, 0).unwrap()
    }

    fn texts(buffer: &LogBuffer) -> Vec<String> {
        buffer
            .lines()
            .iter()
            .map(|line| match line {
                LogLine::Entry { text, .. } => text.clone(),
                LogLine::Gap => "<gap>".to_string(),
                LogLine::Separator => "<separator>".to_string(),
            })
            .collect()
    }

    fn poll(buffer: &mut LogBuffer, body: &str) {
        let query = buffer.next_query();
        buffer.ingest(body, query, at(0));
    }

    #[test]
    fn timestamps_with_any_fraction_length_are_parsed() {
        let (short, text) = parse_line("2026-09-23T10:00:00.1Z hello world").unwrap();
        assert_eq!(text, "hello world");
        let (long, _) = parse_line("2026-09-23T10:00:00.100000000Z x").unwrap();
        assert_eq!(short, long);
        let (whole, _) = parse_line("2026-09-23T10:00:00Z x").unwrap();
        assert!(whole < short);
        assert!(parse_line("Service is not ready yet").is_none());
    }

    #[test]
    fn the_first_poll_asks_for_a_tail_and_later_ones_overlap() {
        let mut buffer = LogBuffer::default();
        assert_eq!(
            buffer.next_query(),
            LogQuery {
                tail: 500,
                since: None
            }
        );
        poll(&mut buffer, "2026-09-23T10:00:07.25Z started\n");
        assert_eq!(
            buffer.next_query(),
            LogQuery {
                tail: 1000,
                since: Some(BASE + 7 - 5)
            }
        );
    }

    #[test]
    fn overlapping_lines_are_not_repeated() {
        let mut buffer = LogBuffer::default();
        poll(
            &mut buffer,
            "2026-09-23T10:00:01Z a\n2026-09-23T10:00:02Z b\n",
        );
        poll(
            &mut buffer,
            "2026-09-23T10:00:01Z a\n2026-09-23T10:00:02Z b\n2026-09-23T10:00:03Z c\n",
        );
        assert_eq!(texts(&buffer), vec!["a", "b", "c"]);
    }

    #[test]
    fn identical_lines_sharing_a_timestamp_are_kept() {
        let mut buffer = LogBuffer::default();
        poll(
            &mut buffer,
            "2026-09-23T10:00:01Z ping\n2026-09-23T10:00:01Z ping\n",
        );
        poll(
            &mut buffer,
            "2026-09-23T10:00:01Z ping\n2026-09-23T10:00:01Z ping\n2026-09-23T10:00:01Z ping\n",
        );
        assert_eq!(texts(&buffer), vec!["ping", "ping", "ping"]);
    }

    #[test]
    fn a_full_response_that_starts_after_since_marks_a_gap() {
        let mut buffer = LogBuffer::default();
        poll(&mut buffer, "2026-09-23T10:00:01Z a\n");
        let body: String = (0..1000)
            .map(|index| format!("2026-09-23T10:00:30Z line {index}\n"))
            .collect();
        poll(&mut buffer, &body);
        assert_eq!(texts(&buffer)[1], "<gap>");
    }

    #[test]
    fn a_gap_is_found_within_the_first_second_after_since() {
        let mut buffer = LogBuffer::default();
        poll(&mut buffer, "2026-09-23T10:00:10Z a\n");
        let body: String = (0..1000)
            .map(|index| format!("2026-09-23T10:00:05.5Z line {index}\n"))
            .collect();
        poll(&mut buffer, &body);
        assert_eq!(texts(&buffer)[1], "<gap>");
    }

    #[test]
    fn only_the_awaited_reply_is_accepted() {
        let mut buffer = LogBuffer::default();
        let first = buffer.start_fetch();
        let stale = LogQuery {
            tail: 1000,
            since: Some(BASE),
        };
        assert!(!buffer.finish_fetch(stale));
        assert!(buffer.finish_fetch(first));
        assert!(!buffer.finish_fetch(first), "a duplicate reply is dropped");
    }

    #[test]
    fn a_response_below_the_tail_has_no_gap() {
        let mut buffer = LogBuffer::default();
        poll(&mut buffer, "2026-09-23T10:00:01Z a\n");
        poll(&mut buffer, "2026-09-23T10:00:30Z b\n");
        assert_eq!(texts(&buffer), vec!["a", "b"]);
    }

    #[test]
    fn clearing_the_view_keeps_the_cursor() {
        let mut buffer = LogBuffer::default();
        poll(&mut buffer, "2026-09-23T10:00:09Z a\n");
        buffer.clear_view();
        assert!(buffer.lines().is_empty());
        assert_eq!(buffer.next_query().since, Some(BASE + 9 - 5));

        poll(
            &mut buffer,
            "2026-09-23T10:00:09Z a\n2026-09-23T10:00:10Z b\n",
        );
        assert_eq!(texts(&buffer), vec!["b"]);
    }

    #[test]
    fn eviction_keeps_the_cursor_and_the_capacity() {
        let mut buffer = LogBuffer::default();
        let body: String = (0..RING_CAPACITY + 10)
            .map(|index| format!("2026-09-23T10:00:{:02}Z line {index}\n", index % 60))
            .collect();
        buffer.ingest(
            &body,
            LogQuery {
                tail: 100_000,
                since: None,
            },
            at(0),
        );
        assert_eq!(buffer.lines().len(), RING_CAPACITY);
        assert_eq!(buffer.next_query().since, Some(BASE + 59 - 5));
    }

    #[test]
    fn a_message_without_timestamps_becomes_the_status() {
        let mut buffer = LogBuffer::default();
        poll(&mut buffer, "Service is not ready yet. Try again shortly.");
        assert!(buffer.lines().is_empty());
        assert_eq!(
            buffer.status_message.as_deref(),
            Some("Service is not ready yet. Try again shortly.")
        );
        assert_eq!(buffer.next_query().since, None);

        poll(&mut buffer, "2026-09-23T10:00:01Z up\n");
        assert_eq!(buffer.status_message, None);
    }

    #[test]
    fn an_empty_body_keeps_the_lines_and_clears_a_stale_message() {
        let mut buffer = LogBuffer::default();
        poll(&mut buffer, "2026-09-23T10:00:01Z a\n");
        buffer.status_message = Some("Unable to fetch logs".to_string());
        poll(&mut buffer, "  \n");
        assert_eq!(texts(&buffer), vec!["a"]);
        assert_eq!(buffer.status_message, None);
    }

    #[test]
    fn scrolling_up_stops_following_and_g_resumes() {
        let mut buffer = LogBuffer::default();
        poll(
            &mut buffer,
            "2026-09-23T10:00:01Z a\n2026-09-23T10:00:02Z b\n",
        );
        buffer.scroll_up(1);
        assert!(!buffer.follow);

        poll(&mut buffer, "2026-09-23T10:00:03Z c\n");
        assert_eq!(buffer.scroll_from_bottom, 2, "the view stays put");

        buffer.follow_tail();
        assert!(buffer.follow);
        assert_eq!(buffer.scroll_from_bottom, 0);
    }

    #[test]
    fn returning_after_thirty_seconds_restarts_with_a_separator() {
        let mut buffer = LogBuffer::default();
        buffer.ingest(
            "2026-09-23T10:00:01Z a\n",
            LogQuery {
                tail: 500,
                since: None,
            },
            at(0),
        );
        buffer.resume(at(20));
        assert!(buffer.next_query().since.is_some());

        buffer.resume(at(31));
        assert_eq!(buffer.next_query().since, None);
        assert_eq!(texts(&buffer), vec!["a", "<separator>"]);
    }

    #[test]
    fn search_jumps_between_matches() {
        let mut buffer = LogBuffer::default();
        poll(
            &mut buffer,
            "2026-09-23T10:00:01Z error one\n2026-09-23T10:00:02Z ok\n2026-09-23T10:00:03Z ERROR two\n2026-09-23T10:00:04Z ok\n",
        );
        buffer.search = "error".to_string();
        buffer.apply_search();
        assert_eq!(buffer.current_match, Some(2));
        assert_eq!(buffer.scroll_from_bottom, 1);

        buffer.step_match(false);
        assert_eq!(buffer.current_match, Some(0));
        buffer.step_match(false);
        assert_eq!(buffer.current_match, Some(0));
        buffer.step_match(true);
        assert_eq!(buffer.current_match, Some(2));
    }
}
