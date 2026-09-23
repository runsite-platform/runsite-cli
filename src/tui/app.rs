use super::action::{Action, Effect, FetchError, Payload, ProfileEntry, Request};
use super::dashboard::{DashboardState, Pane, ProjectChoice};
use super::login::{LoginState, LoginTab};
use super::poller::{Cadence, Poller};
use super::status::Tone;
use crate::api::CurrentUser;
use chrono::{DateTime, TimeDelta, Utc};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use std::collections::BTreeMap;
use uuid::Uuid;

const TOAST_SECONDS: i64 = 4;
pub const RATE_LIMIT_MESSAGE: &str = "Rate limit reached, try again in a minute";
pub const INVALID_KEY_NOTICE: &str = "Your API key is invalid or revoked";

#[derive(Debug)]
pub struct Session {
    pub profile: String,
    pub api_url: String,
    /// `RUNSITE_API_TOKEN` overrides the profile's key.
    pub env_token: bool,
    pub has_credentials: bool,
    pub user: Option<CurrentUser>,
    pub key_scope: Option<String>,
}

#[derive(Debug)]
pub enum Screen {
    Dashboard,
    Login(LoginState),
    Blocked { reason: String },
}

#[derive(Debug, Default)]
pub struct ProfilePicker {
    /// `None` while the config is being read.
    pub entries: Option<Vec<ProfileEntry>>,
    pub cursor: usize,
}

#[derive(Debug)]
pub enum Overlay {
    Help,
    ProfilePicker(ProfilePicker),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Toast {
    pub text: String,
    pub tone: Tone,
    pub expires_at: DateTime<Utc>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExitReason {
    Quit,
    EnvTokenRejected,
}

#[derive(Debug)]
pub struct App {
    pub now: DateTime<Utc>,
    pub width: u16,
    pub height: u16,
    pub session: Session,
    /// Bumped on login and profile switch; older results are dropped.
    pub generation: u64,
    pub screen: Screen,
    pub overlay: Option<Overlay>,
    pub dashboard: DashboardState,
    pub toast: Option<Toast>,
    pub poller: Poller,
    pub last_refresh: Option<DateTime<Utc>>,
    /// Latest failed read, shown in the status line until the next success.
    pub last_error: Option<String>,
    /// Project picked by the user per profile, saved to the config on exit.
    pub chosen_projects: BTreeMap<String, Uuid>,
    pub exit: Option<ExitReason>,
}

impl App {
    pub fn new(
        session: Session,
        preferred_project: Option<Uuid>,
        now: DateTime<Utc>,
        (width, height): (u16, u16),
    ) -> Self {
        let screen = if session.has_credentials {
            Screen::Dashboard
        } else {
            Screen::Login(LoginState::default())
        };
        Self {
            now,
            width,
            height,
            session,
            generation: 0,
            screen,
            overlay: None,
            dashboard: DashboardState::new(preferred_project),
            toast: None,
            poller: Poller::default(),
            last_refresh: None,
            last_error: None,
            chosen_projects: BTreeMap::new(),
            exit: None,
        }
    }

    /// The first fetches, before any input arrives.
    pub fn start(&mut self) -> Vec<Effect> {
        self.schedule_fetches(Vec::new())
    }

    pub fn show_toast(&mut self, text: impl Into<String>, tone: Tone) {
        self.toast = Some(Toast {
            text: text.into(),
            tone,
            expires_at: self.now + TimeDelta::seconds(TOAST_SECONDS),
        });
    }

    fn quit(&mut self) -> Vec<Effect> {
        self.exit = Some(ExitReason::Quit);
        vec![Effect::Exit]
    }

    fn desired_polls(&self) -> Vec<(Request, Cadence)> {
        if !matches!(self.screen, Screen::Dashboard) {
            return Vec::new();
        }
        let mut polls = vec![
            (Request::CurrentUser, Cadence::Once),
            (Request::KeyScope, Cadence::Once),
            (Request::Projects, Cadence::seconds(30)),
            // Needed on every dashboard view: the Unassigned row only exists
            // when this list has services without a project.
            (Request::UnassignedServices, Cadence::seconds(30)),
        ];
        // Wait for the list: a project saved in the config may no longer exist.
        if let (Some(_), Some(ProjectChoice::Project(id))) =
            (&self.dashboard.projects, self.dashboard.selected_project)
        {
            polls.push((Request::ProjectDetail(id), Cadence::seconds(10)));
        }
        polls
    }

    fn schedule_fetches(&mut self, mut effects: Vec<Effect>) -> Vec<Effect> {
        if effects.iter().any(|effect| matches!(effect, Effect::Exit)) {
            return effects;
        }
        let desired = self.desired_polls();
        self.poller.sync(&desired, self.now);
        for request in self.poller.take_due(self.now) {
            effects.push(Effect::Fetch {
                generation: self.generation,
                request,
            });
        }
        effects
    }

    fn handle(&mut self, action: Action) -> Vec<Effect> {
        match action {
            Action::Tick { now } => {
                self.now = now;
                if self
                    .toast
                    .as_ref()
                    .is_some_and(|toast| toast.expires_at <= now)
                {
                    self.toast = None;
                }
                Vec::new()
            }
            Action::Resize { width, height } => {
                self.width = width;
                self.height = height;
                Vec::new()
            }
            Action::Key(key) => self.handle_key(key),
            Action::Loaded {
                generation,
                request,
                result,
            } => {
                if generation != self.generation || !self.poller.is_active(&request) {
                    return Vec::new();
                }
                match result {
                    Ok(payload) => {
                        self.poller.record_success(request, self.now);
                        self.last_refresh = Some(self.now);
                        self.last_error = None;
                        self.apply_payload(payload);
                        Vec::new()
                    }
                    Err(error) => self.handle_fetch_error(request, error),
                }
            }
            Action::LoggedIn { generation, result } => self.handle_login_result(generation, result),
            Action::ProfilesListed(entries) => {
                if let Some(Overlay::ProfilePicker(picker)) = &mut self.overlay {
                    picker.cursor = entries
                        .iter()
                        .position(|entry| entry.name == self.session.profile)
                        .unwrap_or(0);
                    picker.entries = Some(entries);
                }
                Vec::new()
            }
            Action::Terminate => self.quit(),
        }
    }

    fn apply_payload(&mut self, payload: Payload) {
        match payload {
            Payload::CurrentUser(user) => self.session.user = Some(user),
            Payload::KeyScope(identity) => self.session.key_scope = Some(identity.scope),
            Payload::Projects(projects) => {
                self.dashboard.projects = Some(projects);
                self.dashboard.normalize_selection();
            }
            Payload::ProjectDetail(detail) => {
                self.dashboard.details.insert(detail.id, detail);
                self.dashboard.normalize_selection();
            }
            Payload::UnassignedServices(services) => {
                self.dashboard.unassigned = Some(
                    services
                        .into_iter()
                        .filter(|service| service.project_id.is_none())
                        .collect(),
                );
                self.dashboard.normalize_selection();
            }
        }
    }

    fn handle_fetch_error(&mut self, request: Request, error: FetchError) -> Vec<Effect> {
        match error {
            FetchError::Unauthorized { .. } => {
                if self.session.env_token {
                    self.exit = Some(ExitReason::EnvTokenRejected);
                    return vec![Effect::Exit];
                }
                self.generation += 1;
                self.session.has_credentials = false;
                self.overlay = None;
                self.poller.clear();
                self.screen = Screen::Login(LoginState::with_notice(INVALID_KEY_NOTICE));
            }
            FetchError::Blocked { reason } => {
                self.overlay = None;
                self.poller.clear();
                self.screen = Screen::Blocked { reason };
            }
            FetchError::RateLimited => {
                self.poller.record_failure(request, self.now, true);
                self.show_toast(RATE_LIMIT_MESSAGE, Tone::Bad);
            }
            FetchError::Offline { .. } => {
                self.poller.record_failure(request, self.now, true);
            }
            FetchError::NotFound { .. } => {
                self.poller.record_failure(request, self.now, false);
                self.handle_not_found(request);
            }
            FetchError::Forbidden { message } | FetchError::Rejected { message, .. } => {
                self.poller.record_failure(request, self.now, false);
                self.last_error = Some(message.clone());
                self.show_toast(message, Tone::Bad);
            }
        }
        Vec::new()
    }

    fn handle_not_found(&mut self, request: Request) {
        if let Request::ProjectDetail(id) = request {
            // Drop it from the list too, so it is not polled again.
            self.dashboard.details.remove(&id);
            if let Some(projects) = &mut self.dashboard.projects {
                projects.retain(|project| project.id != id);
            }
            self.dashboard.normalize_selection();
            self.show_toast("No longer exists", Tone::Bad);
        }
    }

    fn handle_login_result(
        &mut self,
        generation: u64,
        result: Result<String, FetchError>,
    ) -> Vec<Effect> {
        if generation != self.generation {
            return Vec::new();
        }
        let Screen::Login(login) = &mut self.screen else {
            return Vec::new();
        };
        match result {
            Ok(_email) => {
                // Requests sent with the old key may still answer 401.
                self.generation += 1;
                self.session.has_credentials = true;
                self.session.user = None;
                self.session.key_scope = None;
                self.poller.clear();
                self.screen = Screen::Dashboard;
            }
            Err(FetchError::Blocked { reason }) => {
                self.screen = Screen::Blocked { reason };
            }
            Err(error) => {
                login.submitting = false;
                login.error = Some(error.message());
            }
        }
        Vec::new()
    }

    fn handle_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        if key.kind != KeyEventKind::Press {
            return Vec::new();
        }
        let control = key.modifiers.contains(KeyModifiers::CONTROL);
        if control && key.code == KeyCode::Char('c') {
            return self.quit();
        }
        if control && key.code == KeyCode::Char('z') {
            return Vec::new();
        }
        if self.overlay.is_some() {
            return self.handle_overlay_key(key);
        }
        match self.screen {
            Screen::Blocked { .. } => {
                if key.code == KeyCode::Char('q') {
                    return self.quit();
                }
                Vec::new()
            }
            Screen::Login(ref login) if login.navigating => {
                if let Some(effects) = self.handle_global_key(key) {
                    return effects;
                }
                if let Screen::Login(login) = &mut self.screen {
                    if matches!(key.code, KeyCode::Enter | KeyCode::Tab | KeyCode::Char('i')) {
                        login.navigating = false;
                    }
                }
                Vec::new()
            }
            Screen::Login(_) => self.handle_login_key(key),
            Screen::Dashboard => {
                if self.dashboard.editing_filter.is_some() {
                    self.handle_filter_key(key);
                    return Vec::new();
                }
                match self.handle_global_key(key) {
                    Some(effects) => effects,
                    None => {
                        self.handle_dashboard_key(key);
                        Vec::new()
                    }
                }
            }
        }
    }

    /// Keys shared by every navigation screen. `None` when the key is not global.
    fn handle_global_key(&mut self, key: KeyEvent) -> Option<Vec<Effect>> {
        match key.code {
            KeyCode::Char('q') => Some(self.quit()),
            KeyCode::Char('?') => {
                self.overlay = Some(Overlay::Help);
                Some(Vec::new())
            }
            KeyCode::Char('r') => {
                self.poller.refresh_all(self.now);
                Some(Vec::new())
            }
            KeyCode::Char('P') => {
                if self.session.env_token {
                    self.show_toast(
                        "Profiles are disabled: the env token overrides profiles",
                        Tone::Bad,
                    );
                    return Some(Vec::new());
                }
                self.overlay = Some(Overlay::ProfilePicker(ProfilePicker::default()));
                Some(vec![Effect::ListProfiles])
            }
            _ => None,
        }
    }

    fn handle_overlay_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        let Some(Overlay::ProfilePicker(picker)) = &mut self.overlay else {
            // Any key closes the help overlay.
            self.overlay = None;
            return Vec::new();
        };
        let count = picker.entries.as_ref().map_or(0, Vec::len);
        match key.code {
            KeyCode::Esc => self.overlay = None,
            KeyCode::Char('q') => return self.quit(),
            KeyCode::Up | KeyCode::Char('k') => picker.cursor = picker.cursor.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') if count > 0 => {
                picker.cursor = (picker.cursor + 1).min(count - 1)
            }
            KeyCode::Enter => {
                let chosen = picker
                    .entries
                    .as_ref()
                    .and_then(|entries| entries.get(picker.cursor))
                    .cloned();
                if let Some(entry) = chosen {
                    return self.switch_profile(entry);
                }
            }
            _ => {}
        }
        Vec::new()
    }

    fn switch_profile(&mut self, entry: ProfileEntry) -> Vec<Effect> {
        self.overlay = None;
        self.generation += 1;
        self.poller.clear();
        self.last_refresh = None;
        let preferred = self
            .chosen_projects
            .get(&entry.name)
            .copied()
            .or(entry.current_project_id);
        self.dashboard = DashboardState::new(preferred);
        self.session = Session {
            profile: entry.name.clone(),
            api_url: entry.api_url.clone(),
            env_token: false,
            has_credentials: entry.has_key,
            user: None,
            key_scope: None,
        };
        self.screen = if entry.has_key {
            Screen::Dashboard
        } else {
            Screen::Login(LoginState::default())
        };
        vec![Effect::SwitchProfile {
            profile: entry.name,
            api_url: entry.api_url,
        }]
    }

    fn handle_login_key(&mut self, key: KeyEvent) -> Vec<Effect> {
        let generation = self.generation;
        let Screen::Login(login) = &mut self.screen else {
            return Vec::new();
        };
        if login.submitting {
            return Vec::new();
        }
        let control = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Left if control => login.switch_tab(LoginTab::Password),
            KeyCode::Right if control => login.switch_tab(LoginTab::ApiKey),
            KeyCode::Tab => login.move_focus(true),
            KeyCode::BackTab => login.move_focus(false),
            KeyCode::Esc => {
                let field = login.field_mut(login.focused_field());
                if field.is_empty() {
                    login.navigating = true;
                } else {
                    field.clear();
                }
            }
            KeyCode::Backspace => {
                login.field_mut(login.focused_field()).pop();
            }
            KeyCode::Enter => match login.credentials() {
                Ok(credentials) => {
                    login.submitting = true;
                    login.error = None;
                    return vec![Effect::LogIn {
                        generation,
                        credentials,
                    }];
                }
                Err(message) => login.error = Some(message.to_string()),
            },
            KeyCode::Char(character) if !control => {
                login.field_mut(login.focused_field()).push(character);
                login.error = None;
            }
            _ => {}
        }
        Vec::new()
    }

    fn handle_filter_key(&mut self, key: KeyEvent) {
        let Some(pane) = self.dashboard.editing_filter else {
            return;
        };
        match key.code {
            KeyCode::Esc => {
                let filter = self.dashboard.filter_mut(pane);
                if filter.is_empty() {
                    self.dashboard.editing_filter = None;
                } else {
                    filter.clear();
                }
            }
            KeyCode::Enter => self.dashboard.editing_filter = None,
            KeyCode::Backspace => {
                self.dashboard.filter_mut(pane).pop();
            }
            KeyCode::Char(character) => self.dashboard.filter_mut(pane).push(character),
            _ => {}
        }
        self.dashboard.normalize_selection();
    }

    fn handle_dashboard_key(&mut self, key: KeyEvent) {
        let focus = self.dashboard.focus;
        match key.code {
            KeyCode::Tab
            | KeyCode::Left
            | KeyCode::Right
            | KeyCode::Char('h')
            | KeyCode::Char('l') => {
                self.dashboard.focus = match focus {
                    Pane::Projects => Pane::Resources,
                    Pane::Resources => Pane::Projects,
                };
            }
            KeyCode::Up | KeyCode::Char('k') => self.move_cursor(focus, -1),
            KeyCode::Down | KeyCode::Char('j') => self.move_cursor(focus, 1),
            KeyCode::Enter if focus == Pane::Projects => self.dashboard.focus = Pane::Resources,
            KeyCode::Esc if focus == Pane::Resources => self.dashboard.focus = Pane::Projects,
            KeyCode::Char('/') => self.dashboard.editing_filter = Some(focus),
            _ => {}
        }
    }

    fn move_cursor(&mut self, pane: Pane, delta: isize) {
        match pane {
            Pane::Projects => {
                self.dashboard.move_project(delta);
                if let Some(ProjectChoice::Project(id)) = self.dashboard.selected_project {
                    self.chosen_projects
                        .insert(self.session.profile.clone(), id);
                }
            }
            Pane::Resources => self.dashboard.move_resource(delta),
        }
    }
}

pub fn update(app: &mut App, action: Action) -> Vec<Effect> {
    let effects = app.handle(action);
    app.schedule_fetches(effects)
}
