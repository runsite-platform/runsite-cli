use super::centered;
use crate::tui::status::Tone;
use crate::tui::theme::Theme;
use ratatui::layout::{Alignment, Rect};
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

pub fn render(frame: &mut Frame, area: Rect, reason: &str, theme: &Theme) {
    let mut lines = vec![Line::styled(
        "Your account is blocked",
        theme.tone(Tone::Bad),
    )];
    if !reason.is_empty() {
        lines.push(Line::from(reason.to_string()));
    }
    lines.push(Line::from(""));
    lines.push(Line::styled("Press q to quit", theme.muted()));
    frame.render_widget(
        Paragraph::new(lines)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true }),
        centered(area, area.width.min(70), 5),
    );
}
