use crate::tui::app::App;
use crate::tui::theme::Theme;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn identity(app: &App, theme: &Theme) -> String {
    let session = &app.session;
    let mut parts = Vec::new();
    if let Some(user) = &session.user {
        parts.push(user.email.clone());
    }
    if session.env_token {
        parts.push("env: RUNSITE_API_TOKEN".to_string());
    } else {
        parts.push(format!("profile: {}", session.profile));
    }
    parts.join(theme.separator())
}

pub fn render(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let title = Span::styled(format!("RunSite CLI v{VERSION}"), theme.accent());

    if area.height < 3 {
        let line = Line::from(vec![
            title,
            Span::raw(theme.separator()),
            Span::raw(identity(app, theme)),
            Span::raw(theme.separator()),
            Span::styled(app.session.api_url.clone(), theme.muted()),
        ]);
        frame.render_widget(Paragraph::new(line), area);
        return;
    }

    let [logo_top, logo_bottom] = theme.logo();
    let indent = " ".repeat(logo_top.chars().count() + 3);
    let lines = vec![
        Line::from(vec![
            Span::styled(format!(" {logo_top}  "), theme.accent()),
            title,
        ]),
        Line::from(vec![
            Span::styled(format!(" {logo_bottom}  "), theme.accent()),
            Span::raw(identity(app, theme)),
        ]),
        Line::from(vec![
            Span::raw(indent),
            Span::styled(app.session.api_url.clone(), theme.muted()),
        ]),
    ];
    frame.render_widget(Paragraph::new(lines), area);
}
