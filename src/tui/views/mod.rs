mod blocked;
mod confirm;
mod dashboard;
mod database_detail;
mod header;
mod help;
mod login;
mod profile_picker;
mod service_detail;
#[cfg(test)]
mod snapshot_tests;
mod status_bar;
mod too_small;

use super::app::{App, Overlay, Screen};
use super::theme::Theme;
use chrono::{DateTime, Utc};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::widgets::{Block, Clear};
use ratatui::Frame;

pub const MIN_WIDTH: u16 = 60;
pub const MIN_HEIGHT: u16 = 15;
/// Below this many rows the header collapses to one line.
pub const COMPACT_HEIGHT: u16 = 20;
/// Below this many columns the dashboard shows one pane at a time.
pub const SPLIT_WIDTH: u16 = 100;

pub fn render(frame: &mut Frame, app: &App, theme: &Theme) {
    let area = frame.area();
    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        too_small::render(frame, area, theme);
        return;
    }

    let header_height = if area.height < COMPACT_HEIGHT { 1 } else { 3 };
    let [header_area, body_area, footer_area] = Layout::vertical([
        Constraint::Length(header_height),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(area);

    header::render(frame, header_area, app, theme);
    match &app.screen {
        Screen::Dashboard => dashboard::render(frame, body_area, app, theme),
        Screen::ServiceDetail(detail) => {
            service_detail::render(frame, body_area, app, detail, theme)
        }
        Screen::DatabaseDetail(database) => {
            database_detail::render(frame, body_area, app, database, theme)
        }
        Screen::Login(login) => login::render(frame, body_area, app, login, theme),
        Screen::Blocked { reason } => blocked::render(frame, body_area, reason, theme),
    }
    status_bar::render(frame, footer_area, app, theme);

    match &app.overlay {
        Some(Overlay::Help) => help::render(frame, body_area, app, theme),
        Some(Overlay::Confirm(confirm)) => confirm::render(frame, body_area, confirm, theme),
        Some(Overlay::ProfilePicker(picker)) => {
            profile_picker::render(frame, body_area, app, picker, theme)
        }
        None => {}
    }
}

/// A `width` x `height` box in the middle of `area`, clipped to it.
pub fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    }
}

/// Clear `area` and draw a bordered box titled `title`; returns the inner area.
pub fn modal(frame: &mut Frame, area: Rect, title: &str, theme: &Theme) -> Rect {
    frame.render_widget(Clear, area);
    let block = Block::bordered()
        .border_set(theme.border_set())
        .border_style(theme.border(true))
        .title(format!(" {title} "));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    inner
}

pub fn format_age(now: DateTime<Utc>, then: DateTime<Utc>) -> String {
    let seconds = (now - then).num_seconds().max(0);
    match seconds {
        0..=59 => format!("{seconds}s ago"),
        60..=3599 => format!("{}m ago", seconds / 60),
        _ => format!("{}h ago", seconds / 3600),
    }
}

/// Cut `text` to `width` characters, marking the cut with an ellipsis.
pub fn truncate(text: &str, width: usize, theme: &Theme) -> String {
    if text.chars().count() <= width {
        return text.to_string();
    }
    let ellipsis = theme.ellipsis();
    let keep = width.saturating_sub(ellipsis.chars().count());
    let mut cut: String = text.chars().take(keep).collect();
    cut.push_str(ellipsis);
    cut
}

/// Key names such as `ctrl+←` spelled out in ASCII mode.
pub fn key_label(keys: &str, theme: &Theme) -> String {
    if !theme.ascii {
        return keys.to_string();
    }
    keys.replace('←', "left")
        .replace('→', "right")
        .replace('↑', "up")
        .replace('↓', "down")
}

/// First row to draw so that `selected` stays inside a window of `height` rows.
pub fn scroll_offset(selected: usize, height: usize) -> usize {
    if height == 0 {
        return 0;
    }
    selected.saturating_sub(height - 1)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    pub(crate) fn render_text(app: &App, theme: &Theme, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| render(frame, app, theme)).unwrap();
        // Keep snapshots stable across version bumps; same width, so no shifts.
        let version = env!("CARGO_PKG_VERSION");
        terminal
            .backend()
            .to_string()
            .replace(version, &"x".repeat(version.len()))
    }

    #[test]
    fn ages_are_rounded_to_the_largest_unit() {
        let now = DateTime::from_timestamp(10_000, 0).unwrap();
        let ago = |seconds: i64| DateTime::from_timestamp(10_000 - seconds, 0).unwrap();
        assert_eq!(format_age(now, ago(3)), "3s ago");
        assert_eq!(format_age(now, ago(125)), "2m ago");
        assert_eq!(format_age(now, ago(7300)), "2h ago");
    }

    #[test]
    fn long_names_are_cut_with_an_ellipsis() {
        let theme = Theme::from_env(|_| None);
        assert_eq!(truncate("landing-page", 8, &theme), "landing…");
        assert_eq!(truncate("api", 8, &theme), "api");
    }

    #[test]
    fn the_selected_row_stays_visible() {
        assert_eq!(scroll_offset(2, 5), 0);
        assert_eq!(scroll_offset(9, 5), 5);
        assert_eq!(scroll_offset(0, 0), 0);
    }
}
