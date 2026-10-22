use crate::tui::app::App;
use crate::tui::detail::DatabaseDetailState;
use crate::tui::status::database_look;
use crate::tui::theme::Theme;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;

const LABEL_WIDTH: usize = 11;

fn row(label: &str, value: String, theme: &Theme) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label:<LABEL_WIDTH$}"), theme.muted()),
        Span::raw(value),
    ])
}

pub fn render(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    database: &DatabaseDetailState,
    theme: &Theme,
) {
    let block = Block::bordered()
        .border_set(theme.border_set())
        .border_style(theme.border(true))
        .title(format!(" {} ", database.name));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(detail) = &database.detail else {
        frame.render_widget(
            Paragraph::new(Span::styled(
                format!("Loading{}", theme.ellipsis()),
                theme.muted(),
            )),
            inner,
        );
        return;
    };
    let empty = || theme.empty_value().to_string();
    let look = database_look(&detail.status);
    let mut lines = vec![
        row("Engine", detail.kind.clone(), theme),
        row(
            "Plan",
            detail.plan_name.clone().unwrap_or_else(empty),
            theme,
        ),
        Line::from(vec![
            Span::styled(format!("{:<LABEL_WIDTH$}", "Status"), theme.muted()),
            Span::styled(
                format!("{} {}", theme.glyph(look.glyph, app.now), detail.status),
                theme.tone(look.tone),
            ),
        ]),
        row(
            "Version",
            database.version.clone().unwrap_or_else(empty),
            theme,
        ),
        row(
            "Internal",
            detail.internal_hostname.clone().unwrap_or_else(empty),
            theme,
        ),
        row(
            "External",
            detail.external_hostname.clone().unwrap_or_else(empty),
            theme,
        ),
    ];
    if detail.kind == "postgresql" {
        let running = detail.status == "running";
        let percent = |value: Option<f64>| match value {
            Some(value) if running => format!("{value:.1}%"),
            _ => empty(),
        };
        lines.push(Line::from(""));
        lines.push(row("CPU", percent(detail.cpu_usage_percent), theme));
        lines.push(row("Memory", percent(detail.memory_usage_percent), theme));
    }
    frame.render_widget(Paragraph::new(lines), inner);
}
