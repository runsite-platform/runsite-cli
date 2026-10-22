//! What the service detail commands from `keymap` do.

use super::app::{App, Screen};
use super::detail::{DeploymentView, DetailTab, ServiceDetailState};
use super::keymap::DetailCommand;
use ratatui::crossterm::event::{KeyCode, KeyEvent};

impl App {
    /// Rows of log or build output a PgUp/PgDn moves.
    fn page_size(&self) -> usize {
        (self.height as usize).saturating_sub(10).max(1)
    }

    pub(super) fn run_detail_command(&mut self, command: DetailCommand, key: KeyEvent) {
        let page = self.page_size();
        let now = self.now;
        let Screen::ServiceDetail(detail) = &mut self.screen else {
            return;
        };
        let character = match key.code {
            KeyCode::Char(character) => Some(character),
            _ => None,
        };
        let next_tab = match command {
            DetailCommand::TabByDigit => character.and_then(DetailTab::from_digit),
            DetailCommand::NextTab => Some(detail.tab.next()),
            DetailCommand::PreviousTab => Some(detail.tab.previous()),
            _ => None,
        };
        if let Some(tab) = next_tab {
            if tab == DetailTab::Logs && detail.tab != DetailTab::Logs {
                detail.logs.resume(now);
            }
            detail.tab = tab;
            return;
        }
        if detail.deployment_view.is_some() {
            scroll_deployment(detail, command, page);
            return;
        }

        let logs = &mut detail.logs;
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
                detail.deploy_cursor = detail.deploy_cursor.saturating_sub(1)
            }
            DetailCommand::DeployCursorDown => {
                let count = detail.deployments.as_ref().map_or(0, Vec::len);
                detail.deploy_cursor = (detail.deploy_cursor + 1).min(count.saturating_sub(1));
            }
            DetailCommand::OpenDeployment => {
                if let Some(deployment) = detail.selected_deployment() {
                    detail.deployment_view = Some(DeploymentView {
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
