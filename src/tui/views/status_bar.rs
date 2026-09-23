use super::{format_age, key_label, truncate};
use crate::tui::app::{App, Overlay, Screen};
use crate::tui::dashboard::Pane;
use crate::tui::detail::DetailTab;
use crate::tui::status::Tone;
use crate::tui::theme::Theme;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

fn hints(app: &App) -> Vec<(&'static str, &'static str)> {
    match (&app.overlay, &app.screen) {
        (Some(Overlay::ProfilePicker(_)), _) => {
            vec![("enter", "switch"), ("esc", "close"), ("q", "quit")]
        }
        (Some(Overlay::Help), _) => vec![("any key", "close")],
        (None, Screen::Login(login)) if login.navigating => {
            vec![
                ("enter", "back to the form"),
                ("P", "profile"),
                ("?", "help"),
                ("q", "quit"),
            ]
        }
        (Some(Overlay::Confirm(_)), _) => vec![("y", "confirm"), ("n / esc", "cancel")],
        (None, Screen::Login(_)) => vec![
            ("ctrl+←/→", "tab"),
            ("tab", "next field"),
            ("enter", "log in"),
            ("ctrl+c", "quit"),
        ],
        (None, Screen::Blocked { .. }) => vec![("q", "quit")],
        (None, Screen::DatabaseDetail(_)) => vec![("esc", "back"), ("?", "help"), ("q", "quit")],
        (None, Screen::ServiceDetail(detail)) if detail.logs.editing_search => {
            vec![("enter", "search"), ("esc", "clear / close")]
        }
        (None, Screen::ServiceDetail(detail)) if detail.deployment_view.is_some() => {
            vec![("j/k", "scroll"), ("g/G", "top/bottom"), ("esc", "back")]
        }
        (None, Screen::ServiceDetail(detail)) => match detail.tab {
            DetailTab::Overview => vec![("1-3", "tabs"), ("esc", "back"), ("?", "help")],
            DetailTab::Logs => vec![
                ("/", "search"),
                ("f", "follow"),
                ("w", "wrap"),
                ("c", "clear"),
                ("esc", "back"),
                ("?", "help"),
            ],
            DetailTab::Deploys => vec![("enter", "details"), ("esc", "back"), ("?", "help")],
        },
        (None, Screen::Dashboard) if app.dashboard.editing_filter.is_some() => {
            vec![("enter", "apply"), ("esc", "clear / close")]
        }
        (None, Screen::Dashboard) => {
            let enter = match app.dashboard.focus {
                Pane::Projects => ("enter", "select"),
                Pane::Resources => ("enter", "open"),
            };
            vec![enter, ("/", "filter"), ("?", "help"), ("q", "quit")]
        }
    }
}

/// Action keys that apply here; disabled ones are drawn greyed out.
fn action_hints(app: &App) -> Vec<(&'static str, &'static str, bool)> {
    if app.overlay.is_some() {
        return Vec::new();
    }
    let keys: &[char] = match &app.screen {
        Screen::Dashboard if app.dashboard.editing_filter.is_none() => &['D', 'R', 'S'],
        Screen::ServiceDetail(detail) if detail.deployment_view.is_some() => &['B'],
        Screen::ServiceDetail(detail) => match detail.tab {
            DetailTab::Overview => &['D', 'R', 'S'],
            DetailTab::Deploys => &['B', 'D'],
            DetailTab::Logs => &[],
        },
        Screen::DatabaseDetail(_) => &['S'],
        _ => &[],
    };
    keys.iter()
        .filter_map(|key| {
            let resolved = app.resolve_action(*key)?;
            let (label, meaning) = match (key, &resolved) {
                ('D', _) => ("D", "deploy"),
                ('R', _) => ("R", "restart"),
                ('B', _) => ("B", "rollback"),
                (_, Ok(confirm)) if confirm.verb == "Start" => ("S", "start"),
                (_, Ok(_)) => ("S", "stop"),
                (_, Err(_)) => ("S", "start/stop"),
            };
            Some((label, meaning, resolved.is_ok()))
        })
        .collect()
}

fn freshness(app: &App, theme: &Theme) -> Span<'static> {
    if app.poller.backing_off() {
        return Span::styled("offline ", theme.tone(Tone::Bad));
    }
    if let Some(error) = &app.last_error {
        return Span::styled(
            format!("{} ", truncate(error, 40, theme)),
            theme.tone(Tone::Bad),
        );
    }
    match app.last_refresh {
        Some(then) => Span::styled(
            format!("updated {} ", format_age(app.now, then)),
            theme.muted(),
        ),
        None => Span::raw(""),
    }
}

pub fn render(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let freshness = freshness(app, theme);
    let [left, right] = Layout::horizontal([
        Constraint::Min(0),
        Constraint::Length(freshness.width() as u16),
    ])
    .areas(area);

    let left_line = match &app.toast {
        Some(toast) => Line::styled(format!(" {}", toast.text), theme.tone(toast.tone)),
        None => {
            let mut all: Vec<(&str, &str, bool)> = hints(app)
                .into_iter()
                .map(|(key, meaning)| (key, meaning, true))
                .collect();
            let actions = action_hints(app);
            let insert_at = 1.min(all.len());
            all.splice(insert_at..insert_at, actions);

            // Hints that do not fit are dropped whole, keeping a gap before the freshness text.
            let available = usize::from(left.width).saturating_sub(1);
            let mut spans = vec![Span::raw(" ")];
            let mut used = 1;
            for (index, (key, meaning, enabled)) in all.into_iter().enumerate() {
                let mut entry = Vec::new();
                if index > 0 {
                    entry.push(Span::styled(theme.separator(), theme.muted()));
                }
                if enabled {
                    entry.push(Span::styled(key_label(key, theme), theme.accent()));
                    entry.push(Span::raw(format!(" {meaning}")));
                } else {
                    entry.push(Span::styled(format!("{key} {meaning}"), theme.muted()));
                }
                let entry_width: usize = entry.iter().map(Span::width).sum();
                if used + entry_width > available {
                    break;
                }
                used += entry_width;
                spans.extend(entry);
            }
            Line::from(spans)
        }
    };
    frame.render_widget(Paragraph::new(left_line), left);
    frame.render_widget(Paragraph::new(freshness).alignment(Alignment::Right), right);
}
