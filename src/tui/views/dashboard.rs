use super::{scroll_offset, truncate, SPLIT_WIDTH};
use crate::tui::app::App;
use crate::tui::dashboard::{Pane, ProjectChoice, Resource};
use crate::tui::status::{database_look, service_look, StatusLook, Tone};
use crate::tui::theme::Theme;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Wrap};
use ratatui::Frame;

const NAME_WIDTH: usize = 18;

pub fn render(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let dashboard = &app.dashboard;
    if area.width < SPLIT_WIDTH {
        match dashboard.focus {
            Pane::Projects => render_projects(frame, area, app, theme),
            Pane::Resources => render_resources(frame, area, app, theme),
        }
        return;
    }
    let [projects_area, resources_area] =
        Layout::horizontal([Constraint::Length(32), Constraint::Min(0)]).areas(area);
    render_projects(frame, projects_area, app, theme);
    render_resources(frame, resources_area, app, theme);
}

fn pane_title(app: &App, pane: Pane, label: String, loading: bool, theme: &Theme) -> String {
    let dashboard = &app.dashboard;
    let mut title = format!(" {label} ");
    let filter = dashboard.filter(pane);
    if dashboard.editing_filter == Some(pane) {
        title.push_str(&format!("/{}{} ", filter, theme.cursor()));
    } else if !filter.is_empty() {
        title.push_str(&format!("/{filter} "));
    }
    if loading {
        title.push_str(&format!("{} ", theme.spinner(app.now)));
    }
    title
}

fn pane_block(title: String, focused: bool, theme: &Theme) -> Block<'static> {
    Block::bordered()
        .border_set(theme.border_set())
        .border_style(theme.border(focused))
        .title(title)
}

fn row_style(selected: bool, focused: bool, theme: &Theme) -> Style {
    if selected && focused {
        theme.selected()
    } else {
        Style::default()
    }
}

fn pointer(selected: bool, theme: &Theme) -> String {
    if selected {
        format!("{} ", theme.pointer())
    } else {
        "  ".to_string()
    }
}

fn render_projects(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let dashboard = &app.dashboard;
    let focused = dashboard.focus == Pane::Projects;
    let loading = dashboard.projects.is_none();
    let block = pane_block(
        pane_title(app, Pane::Projects, "Projects".to_string(), loading, theme),
        focused,
        theme,
    );
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = dashboard.project_rows();
    if loading {
        frame.render_widget(
            Paragraph::new(Span::styled(
                format!(" Loading{}", theme.ellipsis()),
                theme.muted(),
            )),
            inner,
        );
        return;
    }
    if rows.is_empty() {
        let message = if dashboard.project_filter.is_empty() {
            " No projects yet. Create one with `runsite project create <name>`."
        } else {
            " Nothing matches the filter."
        };
        frame.render_widget(
            Paragraph::new(Span::styled(message, theme.muted())).wrap(Wrap { trim: false }),
            inner,
        );
        return;
    }

    let selected_index = dashboard.selected_row_index().unwrap_or(0);
    let height = inner.height as usize;
    let count_width = 4;
    let name_width = (inner.width as usize).saturating_sub(2 + count_width + 1);
    let lines: Vec<Line> = rows
        .iter()
        .enumerate()
        .skip(scroll_offset(selected_index, height))
        .take(height)
        .map(|(index, row)| {
            let selected = index == selected_index;
            let name_style = if row.choice == ProjectChoice::Unassigned {
                theme.muted()
            } else {
                Style::default()
            };
            Line::from(vec![
                Span::raw(pointer(selected, theme)),
                Span::styled(
                    format!("{:<name_width$}", truncate(row.name, name_width, theme)),
                    name_style,
                ),
                Span::styled(
                    format!(" {:>count_width$}", row.resource_count),
                    theme.muted(),
                ),
            ])
            .style(row_style(selected, focused, theme))
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), inner);
}

fn resource_line(
    resource: &Resource,
    selected: bool,
    focused: bool,
    app: &App,
    theme: &Theme,
) -> Line<'static> {
    let name = format!(
        "{:<NAME_WIDTH$}",
        truncate(resource.name(), NAME_WIDTH, theme)
    );

    let spans = match resource {
        Resource::Service(service) => {
            let look = service_look(&service.status);
            let instances = match (service.min_instances, service.max_instances) {
                (Some(min), Some(max)) if min != max => {
                    format!("{min}{}{max}", theme.range_dash())
                }
                (Some(min), _) => min.to_string(),
                _ => String::new(),
            };
            vec![
                Span::styled(
                    format!("{} ", theme.glyph(look.glyph, app.now)),
                    theme.tone(look.tone),
                ),
                Span::raw(name),
                Span::styled(format!(" {:<12}", service.status), theme.tone(look.tone)),
                Span::styled(format!(" {instances}"), theme.muted()),
            ]
        }
        Resource::Postgres(database) => {
            let look = database_look(&database.status);
            let version = database.postgres_version.clone().unwrap_or_default();
            database_spans(theme, name, format!("postgres {version}"), look, app)
        }
        Resource::Redis(database) => {
            let look = database_look(&database.status);
            let version = database.redis_version.clone().unwrap_or_default();
            database_spans(theme, name, format!("valkey {version}"), look, app)
        }
    };

    let mut line_spans = vec![Span::raw(pointer(selected, theme))];
    line_spans.extend(spans);
    Line::from(line_spans).style(row_style(selected, focused, theme))
}

fn database_spans(
    theme: &Theme,
    name: String,
    engine: String,
    look: StatusLook,
    app: &App,
) -> Vec<Span<'static>> {
    let name_style = if look.tone == Tone::Muted {
        theme.muted()
    } else {
        Style::default()
    };
    vec![
        Span::styled(format!("{} ", theme.database_marker()), theme.accent()),
        Span::styled(name, name_style),
        Span::styled(format!(" {:<12}", engine.trim_end()), theme.muted()),
        Span::styled(
            format!(" {}", theme.glyph(look.glyph, app.now)),
            theme.tone(look.tone),
        ),
    ]
}

fn render_resources(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let dashboard = &app.dashboard;
    let focused = dashboard.focus == Pane::Resources;
    let label = match dashboard.selected_project_name() {
        Some(name) => format!("Resources{}{}", theme.separator(), name),
        None => "Resources".to_string(),
    };
    let loading = dashboard.resources_loading();
    let block = pane_block(
        pane_title(app, Pane::Resources, label, loading, theme),
        focused,
        theme,
    );
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let resources = dashboard.resources();
    if resources.is_empty() {
        let message = if loading {
            ""
        } else if !dashboard.resource_filter.is_empty() {
            " Nothing matches the filter."
        } else if dashboard.selected_project.is_some() {
            " No resources in this project."
        } else {
            ""
        };
        frame.render_widget(Paragraph::new(Span::styled(message, theme.muted())), inner);
        return;
    }

    let height = inner.height as usize;
    let lines: Vec<Line> = resources
        .iter()
        .enumerate()
        .skip(scroll_offset(dashboard.resource_cursor, height))
        .take(height)
        .map(|(index, resource)| {
            resource_line(
                resource,
                index == dashboard.resource_cursor,
                focused,
                app,
                theme,
            )
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), inner);
}
