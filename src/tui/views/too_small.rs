use super::{centered, MIN_HEIGHT, MIN_WIDTH};
use crate::tui::theme::Theme;
use ratatui::layout::{Alignment, Rect};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub fn render(frame: &mut Frame, area: Rect, theme: &Theme) {
    let lines = vec![
        Line::from("Please enlarge the terminal window"),
        Line::styled(
            format!(
                "at least {MIN_WIDTH}x{MIN_HEIGHT}, now {}x{}",
                area.width, area.height
            ),
            theme.muted(),
        ),
    ];
    frame.render_widget(
        Paragraph::new(lines).alignment(Alignment::Center),
        centered(area, area.width, 2),
    );
}
