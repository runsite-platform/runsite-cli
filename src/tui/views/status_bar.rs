use super::{format_age, key_label, truncate};
use crate::tui::app::App;
use crate::tui::keymap::{self, Command};
use crate::tui::status::Tone;
use crate::tui::theme::Theme;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

/// The current mode's hints in table order. Action keys show their live
/// meaning (start or stop) and are greyed out while disabled; ones that mean
/// nothing right now are left out.
fn hints(app: &App) -> Vec<(&'static str, &'static str, bool)> {
    keymap::bindings(keymap::mode(app))
        .into_iter()
        .filter_map(|binding| match binding.command {
            Command::Action(key) => app
                .action_hint(key)
                .map(|hint| (hint.key, hint.hint, hint.enabled)),
            _ => binding.hint.map(|hint| (hint.key, hint.text, true)),
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
            let all = hints(app);

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
