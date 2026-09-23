use super::{format_age, scroll_offset, truncate};
use crate::api::DeploymentInfo;
use crate::tui::app::App;
use crate::tui::detail::{short_ref, DeploymentView, DetailTab, ServiceDetailState};
use crate::tui::log_buffer::LogLine;
use crate::tui::status::{deployment_look, service_look, Tone};
use crate::tui::theme::Theme;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Wrap};
use ratatui::Frame;

const LABEL_WIDTH: usize = 11;
const SPARKLINE_WIDTH: usize = 30;
const UNICODE_BARS: [&str; 8] = ["▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"];
const ASCII_BARS: [&str; 8] = ["_", ".", ",", "-", "~", "=", "+", "#"];

pub fn sparkline(percentages: &[f64], width: usize, theme: &Theme) -> String {
    let bars = if theme.ascii {
        ASCII_BARS
    } else {
        UNICODE_BARS
    };
    let start = percentages.len().saturating_sub(width);
    percentages[start..]
        .iter()
        .map(|value| {
            let level = (value.clamp(0.0, 100.0) / 100.0 * 7.0).round() as usize;
            bars[level]
        })
        .collect()
}

fn format_mebibytes(bytes: i64) -> String {
    format!("{} MB", bytes / (1024 * 1024))
}

fn label(text: &str, theme: &Theme) -> Span<'static> {
    Span::styled(format!("{text:<LABEL_WIDTH$}"), theme.muted())
}

pub fn render(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    detail: &ServiceDetailState,
    theme: &Theme,
) {
    let look = detail.status().map(service_look);
    let mut title = format!(" {}", detail.name);
    if let (Some(look), Some(status)) = (look, detail.status()) {
        title.push_str(&format!(
            "{}{} {} ",
            theme.separator(),
            theme.glyph(look.glyph, app.now),
            status
        ));
    } else {
        title.push(' ');
    }
    let block = Block::bordered()
        .border_set(theme.border_set())
        .border_style(theme.border(true))
        .title(title);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let [tabs_area, body_area] =
        Layout::vertical([Constraint::Length(2), Constraint::Min(0)]).areas(inner);
    let mut tab_spans = Vec::new();
    for (index, tab) in DetailTab::ALL.iter().enumerate() {
        let text = format!(" {} {} ", index + 1, tab.title());
        if *tab == detail.tab {
            tab_spans.push(Span::styled(text, theme.selected()));
        } else {
            tab_spans.push(Span::styled(text, theme.muted()));
        }
        tab_spans.push(Span::raw(" "));
    }
    frame.render_widget(Paragraph::new(Line::from(tab_spans)), tabs_area);

    if let Some(view) = &detail.deployment_view {
        render_deployment(frame, body_area, app, view, theme);
        return;
    }
    match detail.tab {
        DetailTab::Overview => render_overview(frame, body_area, app, detail, theme),
        DetailTab::Logs => render_logs(frame, body_area, detail, theme),
        DetailTab::Deploys => render_deploys(frame, body_area, app, detail, theme),
    }
}

fn render_overview(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    detail: &ServiceDetailState,
    theme: &Theme,
) {
    let Some(service) = &detail.service else {
        frame.render_widget(
            Paragraph::new(Span::styled(
                format!("Loading{}", theme.ellipsis()),
                theme.muted(),
            )),
            area,
        );
        return;
    };
    let empty = theme.empty_value().to_string();
    let look = service_look(&service.status);
    let mut lines = vec![Line::from(vec![
        label("Status", theme),
        Span::styled(
            format!("{} {}", theme.glyph(look.glyph, app.now), service.status),
            theme.tone(look.tone),
        ),
    ])];

    let url = match (&service.url, service.project_type.as_deref()) {
        (Some(url), _) => url.clone(),
        (None, Some("worker")) => "no public URL".to_string(),
        (None, _) => empty.clone(),
    };
    lines.push(Line::from(vec![label("URL", theme), Span::raw(url)]));

    let source = match (&service.image_ref, &service.github_branch) {
        (Some(image), _) if service.source_type.as_deref() == Some("image") => {
            format!("image {image}")
        }
        (_, Some(branch)) => format!("branch {branch}"),
        _ => empty.clone(),
    };
    lines.push(Line::from(vec![label("Source", theme), Span::raw(source)]));

    let mut instances = match (service.min_instances, service.max_instances) {
        (Some(min), Some(max)) if min != max => format!("{min}{}{max}", theme.range_dash()),
        (Some(min), _) => min.to_string(),
        _ => empty.clone(),
    };
    if let (true, Some(metrics)) = (detail.is_running(), &detail.metrics) {
        instances.push_str(&format!(" (live {})", metrics.instance_count));
    }
    lines.push(Line::from(vec![
        label("Instances", theme),
        Span::raw(instances),
    ]));

    let live = match detail.live_deployment() {
        Some(deployment) => deployment_summary(deployment, app, theme),
        None => empty.clone(),
    };
    lines.push(Line::from(vec![label("Live", theme), Span::raw(live)]));

    let in_progress = detail
        .latest_deployment
        .as_ref()
        .filter(|deployment| deployment_look(&deployment.status).transitional);
    if let Some(deployment) = in_progress {
        lines.push(Line::from(vec![
            label("Deploying", theme),
            Span::styled(
                format!(
                    "{} {} {}",
                    theme.spinner(app.now),
                    deployment.status,
                    short_ref(deployment)
                ),
                theme.warning(),
            ),
        ]));
    }

    lines.push(Line::from(""));
    let running_metrics = detail.metrics.as_ref().filter(|_| detail.is_running());
    let cpu_history: Vec<f64> = detail
        .history
        .iter()
        .map(|point| point.cpu_usage_percent)
        .collect();
    let memory_history: Vec<f64> = detail
        .history
        .iter()
        .map(|point| point.memory_usage_percent)
        .collect();
    let (cpu, memory) = match running_metrics {
        Some(metrics) => (
            format!("{:>5.1}%", metrics.cpu_usage_percent),
            format!(
                "{:>5.1}% ({} / {})",
                metrics.memory_usage_percent,
                format_mebibytes(metrics.memory_usage_bytes),
                format_mebibytes(metrics.memory_limit_bytes)
            ),
        ),
        None => (empty.clone(), empty.clone()),
    };
    lines.push(Line::from(vec![
        label("CPU", theme),
        Span::raw(format!("{cpu:<28} ")),
        Span::styled(
            sparkline(&cpu_history, SPARKLINE_WIDTH, theme),
            theme.accent(),
        ),
    ]));
    lines.push(Line::from(vec![
        label("Memory", theme),
        Span::raw(format!("{memory:<28} ")),
        Span::styled(
            sparkline(&memory_history, SPARKLINE_WIDTH, theme),
            theme.accent(),
        ),
    ]));
    frame.render_widget(Paragraph::new(lines), area);
}

fn deployment_summary(deployment: &DeploymentInfo, app: &App, theme: &Theme) -> String {
    let mut summary = short_ref(deployment);
    if let Some(message) = &deployment.commit_message {
        let first_line = message.lines().next().unwrap_or_default();
        summary.push_str(&format!(" {}", truncate(first_line, 40, theme)));
    }
    if let Some(created) = deployment.created_at {
        summary.push_str(&format!(
            "{}{}",
            theme.separator(),
            format_age(app.now, created)
        ));
    }
    summary
}

fn log_line(
    line: &LogLine,
    is_match: bool,
    is_current: bool,
    offset: usize,
    theme: &Theme,
) -> Line<'static> {
    let (stamp, text) = match line {
        LogLine::Entry { timestamp, text } => (
            timestamp
                .map(|time| format!("{} ", time.format("%H:%M:%S")))
                .unwrap_or_default(),
            text.clone(),
        ),
        LogLine::Gap => {
            let marker = if theme.ascii {
                "... lines skipped ..."
            } else {
                "··· lines skipped ···"
            };
            return Line::styled(marker, theme.warning());
        }
        LogLine::Separator => {
            let marker = if theme.ascii {
                "---- resumed ----"
            } else {
                "──── resumed ────"
            };
            return Line::styled(marker, theme.muted());
        }
    };
    let full = format!("{stamp}{text}");
    let visible: String = full.chars().skip(offset).collect();
    let style = if is_current {
        theme.selected()
    } else if is_match {
        theme.warning()
    } else {
        Style::default()
    };
    let stamp_width = stamp.chars().count().saturating_sub(offset);
    let (stamp_part, text_part): (String, String) = (
        visible.chars().take(stamp_width).collect(),
        visible.chars().skip(stamp_width).collect(),
    );
    Line::from(vec![
        Span::styled(stamp_part, theme.muted()),
        Span::styled(text_part, style),
    ])
}

fn rows_needed(line: &Line, width: usize, wrap: bool) -> usize {
    if !wrap || width == 0 {
        return 1;
    }
    line.width().div_ceil(width).max(1)
}

fn render_logs(frame: &mut Frame, area: Rect, detail: &ServiceDetailState, theme: &Theme) {
    let logs = &detail.logs;
    let [info_area, body_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(area);

    let on_off = |flag: bool| if flag { "on" } else { "off" };
    let mut info = vec![
        Span::styled("Showing logs from the newest instance", theme.muted()),
        Span::styled(
            format!(
                "{}follow {}{}wrap {}",
                theme.separator(),
                on_off(logs.follow),
                theme.separator(),
                on_off(logs.wrap)
            ),
            theme.muted(),
        ),
    ];
    if logs.editing_search || !logs.search.is_empty() {
        let cursor = if logs.editing_search {
            theme.cursor()
        } else {
            ""
        };
        info.push(Span::styled(
            format!("{}/{}{}", theme.separator(), logs.search, cursor),
            theme.accent(),
        ));
    }
    frame.render_widget(Paragraph::new(Line::from(info)), info_area);

    let lines = logs.lines();
    let body_area = match (&logs.status_message, lines.is_empty()) {
        // Keep the lines, but say why no new ones arrive (e.g. after a restart).
        (Some(message), false) => {
            let [message_area, rest] =
                Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(body_area);
            frame.render_widget(
                Paragraph::new(Span::styled(message.clone(), theme.warning())),
                message_area,
            );
            rest
        }
        _ => body_area,
    };
    if lines.is_empty() {
        let message = logs
            .status_message
            .clone()
            .unwrap_or_else(|| format!("Waiting for logs{}", theme.ellipsis()));
        frame.render_widget(
            Paragraph::new(Span::styled(message, theme.muted())).wrap(Wrap { trim: true }),
            body_area,
        );
        return;
    }

    let width = body_area.width as usize;
    let height = body_area.height as usize;
    let end = lines.len() - logs.scroll_from_bottom.min(lines.len());
    let mut rows = 0;
    let mut start = end;
    let offset = if logs.wrap { 0 } else { logs.horizontal_offset };
    let mut rendered = Vec::new();
    while start > 0 {
        let index = start - 1;
        let line = log_line(
            &lines[index],
            logs.is_match(index),
            logs.current_match == Some(index),
            offset,
            theme,
        );
        let needed = rows_needed(&line, width, logs.wrap);
        if rows + needed > height && !rendered.is_empty() {
            break;
        }
        rows += needed;
        rendered.push(line);
        start = index;
    }
    rendered.reverse();
    // Scrolled to (or near) the top: fill the rest of the pane downwards.
    let mut next = end;
    while start == 0 && next < lines.len() {
        let line = log_line(
            &lines[next],
            logs.is_match(next),
            logs.current_match == Some(next),
            offset,
            theme,
        );
        let needed = rows_needed(&line, width, logs.wrap);
        if rows + needed > height {
            break;
        }
        rows += needed;
        rendered.push(line);
        next += 1;
    }
    let mut paragraph = Paragraph::new(rendered);
    if logs.wrap {
        paragraph = paragraph.wrap(Wrap { trim: false });
    }
    frame.render_widget(paragraph, body_area);
}

fn render_deploys(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    detail: &ServiceDetailState,
    theme: &Theme,
) {
    let Some(deployments) = &detail.deployments else {
        frame.render_widget(
            Paragraph::new(Span::styled(
                format!("Loading{}", theme.ellipsis()),
                theme.muted(),
            )),
            area,
        );
        return;
    };
    if deployments.is_empty() {
        frame.render_widget(
            Paragraph::new(Span::styled("No deployments yet.", theme.muted())),
            area,
        );
        return;
    }
    let height = area.height as usize;
    let lines: Vec<Line> = deployments
        .iter()
        .enumerate()
        .skip(scroll_offset(detail.deploy_cursor, height))
        .take(height)
        .map(|(index, deployment)| {
            let selected = index == detail.deploy_cursor;
            let look = deployment_look(&deployment.status);
            let source = match (&deployment.branch, &deployment.image_ref) {
                (_, Some(image)) if deployment.commit_sha.is_none() => image.clone(),
                (Some(branch), _) => branch.clone(),
                _ => String::new(),
            };
            let marker = if deployment.is_live {
                "live"
            } else if deployment.status == "running" {
                "ready"
            } else {
                ""
            };
            let age = deployment
                .created_at
                .map(|created| format_age(app.now, created))
                .unwrap_or_default();
            let pointer = if selected {
                format!("{} ", theme.pointer())
            } else {
                "  ".to_string()
            };
            let line = Line::from(vec![
                Span::raw(pointer),
                Span::styled(
                    format!(
                        "{} {:<13}",
                        theme.glyph(look.glyph, app.now),
                        deployment.status
                    ),
                    theme.tone(look.tone),
                ),
                Span::raw(format!("{:<9}", short_ref(deployment))),
                Span::raw(format!("{:<20}", truncate(&source, 19, theme))),
                Span::styled(format!("{age:<10}"), theme.muted()),
                Span::styled(marker, theme.tone(Tone::Good)),
            ]);
            if selected {
                line.style(theme.selected())
            } else {
                line
            }
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), area);
}

fn render_deployment(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    view: &DeploymentView,
    theme: &Theme,
) {
    let Some(deployment) = &view.detail else {
        frame.render_widget(
            Paragraph::new(Span::styled(
                format!("Loading{}", theme.ellipsis()),
                theme.muted(),
            )),
            area,
        );
        return;
    };
    let look = deployment_look(&deployment.status);
    let mut header = vec![Line::from(vec![
        Span::styled(
            format!("Deployment {}", short_ref(deployment)),
            theme.accent(),
        ),
        Span::raw(theme.separator()),
        Span::styled(
            format!("{} {}", theme.glyph(look.glyph, app.now), deployment.status),
            theme.tone(look.tone),
        ),
        Span::styled(
            if deployment.is_live { "  live" } else { "" },
            theme.tone(Tone::Good),
        ),
    ])];
    if let Some(message) = &deployment.commit_message {
        header.push(Line::from(vec![
            label("Commit", theme),
            Span::raw(message.lines().next().unwrap_or_default().to_string()),
        ]));
    }
    if let Some(error) = &deployment.error_message {
        header.push(Line::from(vec![
            label("Error", theme),
            Span::styled(error.clone(), theme.tone(Tone::Bad)),
        ]));
    }
    header.push(Line::from(""));

    let [header_area, logs_area] =
        Layout::vertical([Constraint::Length(header.len() as u16), Constraint::Min(0)]).areas(area);
    frame.render_widget(Paragraph::new(header), header_area);

    let build_logs: Vec<Line> = match &deployment.build_logs {
        Some(logs) if !logs.is_empty() => logs
            .lines()
            .skip(view.scroll)
            .take(logs_area.height as usize)
            .map(|line| Line::raw(line.to_string()))
            .collect(),
        _ => vec![Line::styled("No build logs.", theme.muted())],
    };
    frame.render_widget(Paragraph::new(build_logs), logs_area);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sparklines_scale_percentages_and_keep_the_newest_points() {
        let theme = Theme::from_env(|_| None);
        assert_eq!(sparkline(&[0.0, 50.0, 100.0], 10, &theme), "▁▅█");
        assert_eq!(sparkline(&[0.0, 50.0, 100.0], 2, &theme), "▅█");
        assert_eq!(sparkline(&[150.0], 10, &theme), "█");
    }
}
