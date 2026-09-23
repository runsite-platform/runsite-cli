use super::{centered, key_label, modal};
use crate::tui::app::{App, Screen};
use crate::tui::detail::DetailTab;
use crate::tui::theme::Theme;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

const KEY_WIDTH: usize = 16;

pub fn help_entries(app: &App) -> Vec<(&'static str, &'static str)> {
    match app.screen {
        Screen::Login(_) => vec![
            ("ctrl+← / ctrl+→", "switch tab"),
            ("tab / shift+tab", "next / previous field"),
            ("enter", "log in"),
            ("esc", "clear the field; on an empty one leave the form"),
            ("q ? P", "quit, help, profiles (after leaving the form)"),
            ("ctrl+c", "quit"),
        ],
        Screen::Blocked { .. } => vec![("q", "quit")],
        Screen::ServiceDetail(ref detail) => {
            let mut entries = vec![
                ("1 2 3 / tab", "switch tab"),
                ("esc", "back"),
                ("r", "refresh now"),
            ];
            if detail.deployment_view.is_some() {
                entries.extend([
                    ("j k / pgup pgdn", "scroll build logs"),
                    ("g / G", "top / bottom"),
                ]);
            } else if detail.tab == DetailTab::Logs {
                entries.extend([
                    ("j k / pgup pgdn", "scroll"),
                    ("g / G", "top / bottom and follow"),
                    ("f", "toggle follow"),
                    ("w", "toggle wrap"),
                    ("← →", "scroll sideways (wrap off)"),
                    ("/ n N", "search, next, previous"),
                    ("c", "clear the view"),
                ]);
            } else if detail.tab == DetailTab::Deploys {
                entries.extend([("j k", "move"), ("enter", "deployment details")]);
            }
            entries.extend([
                ("P", "switch profile"),
                ("?", "this help"),
                ("q / ctrl+c", "quit"),
            ]);
            entries
        }
        Screen::DatabaseDetail(_) => vec![
            ("esc", "back"),
            ("r", "refresh now"),
            ("P", "switch profile"),
            ("?", "this help"),
            ("q / ctrl+c", "quit"),
        ],
        Screen::Dashboard => vec![
            ("↑↓ / j k", "move"),
            ("tab / ←→ / h l", "switch pane"),
            ("enter", "open"),
            ("1 2 3", "open a service on a tab"),
            ("esc", "back to projects"),
            ("/", "filter the focused list"),
            ("r", "refresh now"),
            ("P", "switch profile"),
            ("?", "this help"),
            ("q / ctrl+c", "quit"),
        ],
    }
}

pub fn render(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let entries = help_entries(app);
    let box_area = centered(area, 52, entries.len() as u16 + 4);
    let inner = modal(frame, box_area, "Keys", theme);
    let mut lines: Vec<Line> = entries
        .iter()
        .map(|(keys, meaning)| {
            Line::from(vec![
                Span::styled(
                    format!(" {:<KEY_WIDTH$}", key_label(keys, theme)),
                    theme.accent(),
                ),
                Span::raw(meaning.to_string()),
            ])
        })
        .collect();
    lines.push(Line::from(""));
    lines.push(Line::styled(" any key closes this window", theme.muted()));
    frame.render_widget(Paragraph::new(lines), inner);
}
