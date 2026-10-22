use super::{centered, modal, scroll_offset, truncate};
use crate::tui::app::{App, ProfilePicker};
use crate::tui::theme::Theme;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub fn render(frame: &mut Frame, area: Rect, app: &App, picker: &ProfilePicker, theme: &Theme) {
    let rows = picker.entries.as_ref().map_or(1, Vec::len) as u16;
    let box_area = centered(area, 64, rows + 2);
    let inner = modal(frame, box_area, "Profiles", theme);

    let Some(entries) = &picker.entries else {
        frame.render_widget(
            Paragraph::new(Span::styled(
                format!(" Loading{}", theme.ellipsis()),
                theme.muted(),
            )),
            inner,
        );
        return;
    };

    let height = inner.height as usize;
    let lines: Vec<Line> = entries
        .iter()
        .enumerate()
        .skip(scroll_offset(picker.cursor, height))
        .take(height)
        .map(|(index, entry)| {
            let selected = index == picker.cursor;
            let marker = if selected {
                format!("{} ", theme.pointer())
            } else {
                "  ".to_string()
            };
            let current = if entry.name == app.session.profile {
                " (current)"
            } else {
                ""
            };
            let key_note = if entry.has_key { "" } else { " (no key)" };
            let line = Line::from(vec![
                Span::raw(marker),
                Span::raw(format!("{:<16}", truncate(&entry.name, 16, theme))),
                Span::styled(
                    format!(
                        " {}{current}{key_note}",
                        truncate(&entry.api_url, 30, theme)
                    ),
                    theme.muted(),
                ),
            ]);
            if selected {
                line.style(theme.selected())
            } else {
                line
            }
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), inner);
}
