//! What the service detail commands from `keymap` do.

use super::detail::{DeploymentView, DetailTab, ServiceDetailState};
use super::keymap::DetailCommand;
use chrono::{DateTime, Utc};

impl ServiceDetailState {
    /// `character` is the key pressed, when it is a character; `page` is how
    /// many rows PgUp/PgDn move.
    pub fn run_command(
        &mut self,
        command: DetailCommand,
        character: Option<char>,
        page: usize,
        now: DateTime<Utc>,
    ) {
        let next_tab = match command {
            DetailCommand::TabByDigit => character.and_then(DetailTab::from_digit),
            DetailCommand::NextTab => Some(self.tab.next()),
            DetailCommand::PreviousTab => Some(self.tab.previous()),
            _ => None,
        };
        if let Some(tab) = next_tab {
            if tab == DetailTab::Logs && self.tab != DetailTab::Logs {
                self.logs.resume(now);
            }
            self.tab = tab;
            return;
        }
        if self.deployment_view.is_some() {
            scroll_deployment(self, command, page);
            return;
        }

        let logs = &mut self.logs;
        match command {
            DetailCommand::LineUp => logs.scroll_up(1),
            DetailCommand::LineDown => logs.scroll_down(1),
            DetailCommand::PageUp => logs.scroll_up(page),
            DetailCommand::PageDown => logs.scroll_down(page),
            DetailCommand::Top => logs.jump_to_top(),
            DetailCommand::Bottom => logs.follow_tail(),
            DetailCommand::ToggleFollow => logs.toggle_follow(),
            DetailCommand::ToggleWrap => logs.toggle_wrap(),
            DetailCommand::ClearView => logs.clear_view(),
            DetailCommand::OpenSearch => logs.open_search(),
            DetailCommand::NextMatch => logs.step_match(true),
            DetailCommand::PreviousMatch => logs.step_match(false),
            DetailCommand::ScrollLeft => {
                logs.scroll_sideways(false);
            }
            DetailCommand::ScrollRight => {
                logs.scroll_sideways(true);
            }
            DetailCommand::SearchEscape => logs.escape_search(),
            DetailCommand::SearchApply => logs.apply_search(),
            DetailCommand::SearchErase => logs.erase_search(),
            DetailCommand::SearchType => {
                if let Some(character) = character {
                    logs.type_search(character);
                }
            }
            DetailCommand::DeployCursorUp => {
                self.deploy_cursor = self.deploy_cursor.saturating_sub(1)
            }
            DetailCommand::DeployCursorDown => {
                let count = self.deployments.as_ref().map_or(0, Vec::len);
                self.deploy_cursor = (self.deploy_cursor + 1).min(count.saturating_sub(1));
            }
            DetailCommand::OpenDeployment => {
                if let Some(deployment) = self.selected_deployment() {
                    self.deployment_view = Some(DeploymentView {
                        id: deployment.id,
                        detail: Some(deployment.clone()),
                        scroll: 0,
                    });
                }
            }
            DetailCommand::TabByDigit
            | DetailCommand::NextTab
            | DetailCommand::PreviousTab
            | DetailCommand::CloseDeployment => {}
        }
    }
}

fn scroll_deployment(detail: &mut ServiceDetailState, command: DetailCommand, page: usize) {
    if command == DetailCommand::CloseDeployment {
        detail.deployment_view = None;
        return;
    }
    let Some(view) = &mut detail.deployment_view else {
        return;
    };
    let line_count = view
        .detail
        .as_ref()
        .and_then(|deployment| deployment.build_logs.as_ref())
        .map_or(0, |logs| logs.lines().count());
    let last = line_count.saturating_sub(1);
    view.scroll = match command {
        DetailCommand::LineUp => view.scroll.saturating_sub(1),
        DetailCommand::LineDown => (view.scroll + 1).min(last),
        DetailCommand::PageUp => view.scroll.saturating_sub(page),
        DetailCommand::PageDown => (view.scroll + page).min(last),
        DetailCommand::Top => 0,
        DetailCommand::Bottom => last,
        _ => view.scroll,
    };
}
