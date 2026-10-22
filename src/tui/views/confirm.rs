use super::{centered, modal};
use crate::tui::action_keys::Confirm;
use crate::tui::theme::Theme;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

pub fn render(frame: &mut Frame, area: Rect, confirm: &Confirm, theme: &Theme) {
    let box_area = centered(area, 60, 6);
    let inner = modal(frame, box_area, "Confirm", theme);
    let lines = vec![
        Line::from(vec![
            Span::raw(format!(" {} ", confirm.verb)),
            Span::styled(
                confirm.target.clone(),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw(confirm.suffix.clone()),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled(" y", theme.accent()),
            Span::raw(" confirm"),
            Span::styled(theme.separator(), theme.muted()),
            Span::styled("n / esc", theme.accent()),
            Span::raw(" cancel"),
        ]),
    ];
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}
