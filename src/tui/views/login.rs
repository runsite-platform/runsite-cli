use super::centered;
use crate::tui::app::App;
use crate::tui::login::{LoginField, LoginState, LoginTab};
use crate::tui::status::Tone;
use crate::tui::theme::Theme;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Wrap};
use ratatui::Frame;

const LABEL_WIDTH: usize = 10;

fn tab_span(label: &str, active: bool, theme: &Theme) -> Span<'static> {
    if active {
        Span::styled(format!(" {label} "), theme.selected())
    } else {
        Span::styled(format!(" {label} "), theme.muted())
    }
}

fn field_line(
    login: &LoginState,
    field: LoginField,
    label: &str,
    value: String,
    theme: &Theme,
) -> Line<'static> {
    let focused = login.focused_field() == field;
    let mut spans = vec![Span::styled(
        format!("{label:<LABEL_WIDTH$}"),
        if focused {
            theme.accent()
        } else {
            Style::default()
        },
    )];
    spans.push(Span::styled(
        value,
        Style::default().add_modifier(Modifier::UNDERLINED),
    ));
    if focused {
        spans.push(Span::styled(theme.cursor(), theme.accent()));
    }
    Line::from(spans)
}

fn masked(text: &str) -> String {
    "*".repeat(text.chars().count())
}

pub fn render(frame: &mut Frame, area: Rect, app: &App, login: &LoginState, theme: &Theme) {
    let form_area = centered(area, 64, 12);
    let block = Block::bordered()
        .border_set(theme.border_set())
        .border_style(theme.border(true))
        .title(format!(
            " Log in{}profile: {} ",
            theme.separator(),
            app.session.profile
        ));
    let inner = block.inner(form_area);
    frame.render_widget(block, form_area);

    let mut lines = Vec::new();
    if let Some(notice) = &login.notice {
        lines.push(Line::styled(notice.clone(), theme.warning()));
    }
    lines.push(Line::from(vec![
        tab_span("Email + password", login.tab == LoginTab::Password, theme),
        Span::raw("  "),
        tab_span("API key", login.tab == LoginTab::ApiKey, theme),
    ]));
    lines.push(Line::from(""));
    match login.tab {
        LoginTab::Password => {
            lines.push(field_line(
                login,
                LoginField::Email,
                "Email",
                login.email.clone(),
                theme,
            ));
            lines.push(field_line(
                login,
                LoginField::Password,
                "Password",
                masked(&login.password),
                theme,
            ));
        }
        LoginTab::ApiKey => {
            lines.push(field_line(
                login,
                LoginField::ApiKey,
                "API key",
                masked(&login.api_key),
                theme,
            ));
        }
    }
    lines.push(Line::from(""));
    if login.submitting {
        lines.push(Line::styled(
            format!("{} Logging in{}", theme.spinner(app.now), theme.ellipsis()),
            theme.muted(),
        ));
    } else if let Some(error) = &login.error {
        lines.push(Line::styled(error.clone(), theme.tone(Tone::Bad)));
    }
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
}
