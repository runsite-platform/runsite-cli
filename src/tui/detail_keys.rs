//! Key handling for the service detail screen.

use super::app::{App, Screen};
use super::detail::{DeploymentView, DetailTab, ServiceDetailState};
use ratatui::crossterm::event::{KeyCode, KeyEvent};

const HORIZONTAL_STEP: usize = 8;

impl App {
    /// Rows of log or build output a PgUp/PgDn moves.
    fn page_size(&self) -> usize {
        (self.height as usize).saturating_sub(10).max(1)
    }

    fn service_detail_mut(&mut self) -> Option<&mut ServiceDetailState> {
        match &mut self.screen {
            Screen::ServiceDetail(detail) => Some(detail),
            _ => None,
        }
    }

    pub(super) fn handle_search_key(&mut self, key: KeyEvent) {
        let Some(detail) = self.service_detail_mut() else {
            return;
        };
        let logs = &mut detail.logs;
        match key.code {
            KeyCode::Esc if logs.search.is_empty() => logs.editing_search = false,
            KeyCode::Esc => {
                logs.search.clear();
                logs.current_match = None;
            }
            KeyCode::Enter => logs.apply_search(),
            KeyCode::Backspace => {
                logs.search.pop();
            }
            KeyCode::Char(character) => logs.search.push(character),
            _ => {}
        }
    }

    pub(super) fn handle_service_key(&mut self, key: KeyEvent) {
        let page = self.page_size();
        let now = self.now;
        let Some(detail) = self.service_detail_mut() else {
            return;
        };

        if detail.deployment_view.is_some() {
            if key.code == KeyCode::Esc {
                detail.deployment_view = None;
            } else {
                scroll_deployment(detail, key, page);
            }
            return;
        }

        let horizontal_scroll = detail.tab == DetailTab::Logs && !detail.logs.wrap;
        let next_tab = match key.code {
            KeyCode::Char(digit @ '1'..='3') => DetailTab::from_digit(digit),
            KeyCode::Tab => Some(detail.tab.next()),
            KeyCode::BackTab => Some(detail.tab.previous()),
            KeyCode::Right | KeyCode::Char('l') if !horizontal_scroll => Some(detail.tab.next()),
            KeyCode::Left | KeyCode::Char('h') if !horizontal_scroll => Some(detail.tab.previous()),
            _ => None,
        };
        if let Some(tab) = next_tab {
            if tab == DetailTab::Logs && detail.tab != DetailTab::Logs {
                detail.logs.resume(now);
            }
            detail.tab = tab;
            return;
        }

        if key.code == KeyCode::Esc {
            self.screen = Screen::Dashboard;
            return;
        }

        match detail.tab {
            DetailTab::Overview => {}
            DetailTab::Logs => handle_logs_key(detail, key, page),
            DetailTab::Deploys => handle_deploys_key(detail, key),
        }
    }
}

fn handle_logs_key(detail: &mut ServiceDetailState, key: KeyEvent, page: usize) {
    let logs = &mut detail.logs;
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => logs.scroll_up(1),
        KeyCode::Down | KeyCode::Char('j') => logs.scroll_down(1),
        KeyCode::PageUp => logs.scroll_up(page),
        KeyCode::PageDown => logs.scroll_down(page),
        KeyCode::Char('g') => logs.jump_to_top(),
        KeyCode::Char('G') => logs.follow_tail(),
        KeyCode::Char('f') => {
            if logs.follow {
                logs.follow = false;
            } else {
                logs.follow_tail();
            }
        }
        KeyCode::Char('w') => {
            logs.wrap = !logs.wrap;
            logs.horizontal_offset = 0;
        }
        KeyCode::Char('c') => logs.clear_view(),
        KeyCode::Char('/') => logs.editing_search = true,
        KeyCode::Char('n') => logs.step_match(true),
        KeyCode::Char('N') => logs.step_match(false),
        KeyCode::Left if !logs.wrap => {
            logs.horizontal_offset = logs.horizontal_offset.saturating_sub(HORIZONTAL_STEP)
        }
        KeyCode::Right if !logs.wrap => logs.horizontal_offset += HORIZONTAL_STEP,
        _ => {}
    }
}

fn handle_deploys_key(detail: &mut ServiceDetailState, key: KeyEvent) {
    let count = detail.deployments.as_ref().map_or(0, Vec::len);
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => {
            detail.deploy_cursor = detail.deploy_cursor.saturating_sub(1)
        }
        KeyCode::Down | KeyCode::Char('j') if count > 0 => {
            detail.deploy_cursor = (detail.deploy_cursor + 1).min(count - 1)
        }
        KeyCode::Enter => {
            if let Some(deployment) = detail.selected_deployment() {
                detail.deployment_view = Some(DeploymentView {
                    id: deployment.id,
                    detail: Some(deployment.clone()),
                    scroll: 0,
                });
            }
        }
        _ => {}
    }
}

fn scroll_deployment(detail: &mut ServiceDetailState, key: KeyEvent, page: usize) {
    let Some(view) = &mut detail.deployment_view else {
        return;
    };
    let line_count = view
        .detail
        .as_ref()
        .and_then(|deployment| deployment.build_logs.as_ref())
        .map_or(0, |logs| logs.lines().count());
    let last = line_count.saturating_sub(1);
    view.scroll = match key.code {
        KeyCode::Up | KeyCode::Char('k') => view.scroll.saturating_sub(1),
        KeyCode::Down | KeyCode::Char('j') => (view.scroll + 1).min(last),
        KeyCode::PageUp => view.scroll.saturating_sub(page),
        KeyCode::PageDown => (view.scroll + page).min(last),
        KeyCode::Char('g') => 0,
        KeyCode::Char('G') => last,
        _ => view.scroll,
    };
}
