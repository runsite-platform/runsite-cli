use super::{centered, key_label, modal};
use crate::tui::app::App;
use crate::tui::keymap::help_entries;
use crate::tui::theme::Theme;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

const KEY_WIDTH: usize = 16;

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
