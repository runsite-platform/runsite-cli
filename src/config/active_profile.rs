use super::{read_from, update_profile_at, Profile};
use anyhow::Result;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

/// The profile this process works as: its API URL, credentials and selected
/// project. Clones share state, so an API key stored after a login is seen by
/// every `ApiClient` built from this profile.
///
/// Every change is written to the config file first (re-reading it, so edits
/// by other `runsite` processes survive) and then mirrored in memory.
#[derive(Clone)]
pub struct ActiveProfile {
    state: Arc<Mutex<State>>,
}

struct State {
    name: String,
    profile: Profile,
    env_token: Option<String>,
    /// `None` keeps changes in memory only.
    path: Option<PathBuf>,
}

impl ActiveProfile {
    /// The profile named by `profile_override`, else the file's `current_profile`.
    /// `env_token` overrides its saved key.
    pub(super) fn from_file(
        path: PathBuf,
        profile_override: Option<&str>,
        env_token: Option<String>,
    ) -> Result<Self> {
        let config = read_from(&path)?;
        let name = profile_override
            .map(str::to_string)
            .unwrap_or(config.current_profile);
        let profile = config.profiles.get(&name).cloned().unwrap_or_default();
        Ok(Self::new(name, profile, env_token, Some(path)))
    }

    #[cfg(test)]
    pub fn in_memory(name: &str, profile: Profile, env_token: Option<String>) -> Self {
        Self::new(name.to_string(), profile, env_token, None)
    }

    fn new(
        name: String,
        profile: Profile,
        env_token: Option<String>,
        path: Option<PathBuf>,
    ) -> Self {
        Self {
            state: Arc::new(Mutex::new(State {
                name,
                profile,
                env_token,
                path,
            })),
        }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn name(&self) -> String {
        self.state().name.clone()
    }

    pub fn api_url(&self) -> String {
        self.state().profile.api_url.clone()
    }

    /// The bearer token for API calls: `RUNSITE_API_TOKEN`, else the saved key.
    pub fn token(&self) -> Option<String> {
        let state = self.state();
        state
            .env_token
            .clone()
            .or_else(|| state.profile.api_key.clone())
    }

    pub fn uses_env_token(&self) -> bool {
        self.state().env_token.is_some()
    }

    pub fn is_authenticated(&self) -> bool {
        self.token().is_some()
    }

    /// The project selected with `runsite project use`, if any.
    pub fn current_project_id(&self) -> Option<Uuid> {
        self.state().profile.selected_project()
    }

    pub fn store_api_key(&self, api_key: String) -> Result<()> {
        self.change(|profile| profile.api_key = Some(api_key.clone()))
    }

    /// Only forgets the key locally; it stays valid on the server.
    pub fn forget_api_key(&self) -> Result<()> {
        self.change(|profile| profile.api_key = None)
    }

    pub fn select_project(&self, project_id: Uuid) -> Result<()> {
        self.change(|profile| profile.select_project(Some(project_id)))
    }

    /// Returns false when no project was selected.
    pub fn clear_project(&self) -> Result<bool> {
        if self.state().profile.current_project_id.is_none() {
            return Ok(false);
        }
        self.change(|profile| profile.select_project(None))?;
        Ok(true)
    }

    pub fn set_api_url(&self, url: &str) -> Result<()> {
        let url = url.trim_end_matches('/').to_string();
        self.change(|profile| profile.api_url = url.clone())
    }

    fn change(&self, edit: impl Fn(&mut Profile)) -> Result<()> {
        let mut state = self.state();
        match state.path.clone() {
            Some(path) => {
                let name = state.name.clone();
                state.profile = update_profile_at(&path, &name, edit)?;
            }
            None => edit(&mut state.profile),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    const PROJECT_ID: &str = "ca83c4a9-1517-4fe3-93aa-bbdab4eaee23";

    fn write_config(path: &Path) {
        std::fs::write(
            path,
            format!(
                r#"
current_profile = "default"

[profiles.default]
api_url = "https://api.runsite.app"
api_key = "ak_live_default"
current_project_id = "{PROJECT_ID}"

[profiles.staging]
api_url = "https://staging.runsite.app"
"#
            ),
        )
        .unwrap();
    }

    fn saved_profile(path: &Path, name: &str) -> Profile {
        read_from(path).unwrap().profiles[name].clone()
    }

    #[test]
    fn the_file_names_the_active_profile() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        write_config(&path);

        let active = ActiveProfile::from_file(path, None, None).unwrap();
        assert_eq!(active.name(), "default");
        assert_eq!(active.token().as_deref(), Some("ak_live_default"));
        assert_eq!(
            active.current_project_id(),
            Some(Uuid::parse_str(PROJECT_ID).unwrap())
        );
    }

    #[test]
    fn a_profile_override_wins_and_is_not_saved() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        write_config(&path);

        let active = ActiveProfile::from_file(path.clone(), Some("staging"), None).unwrap();
        assert_eq!(active.api_url(), "https://staging.runsite.app");
        assert!(!active.is_authenticated());

        active.store_api_key("ak_live_staging".to_string()).unwrap();
        assert_eq!(read_from(&path).unwrap().current_profile, "default");
    }

    #[test]
    fn a_missing_file_gives_the_default_profile() {
        let dir = tempfile::tempdir().unwrap();
        let active = ActiveProfile::from_file(dir.path().join("config.toml"), None, None).unwrap();
        assert_eq!(active.name(), "default");
        assert_eq!(active.api_url(), Profile::default().api_url);
        assert_eq!(active.current_project_id(), None);
    }

    #[test]
    fn the_env_token_wins_over_the_saved_key() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        write_config(&path);

        let active =
            ActiveProfile::from_file(path, Some("staging"), Some("ak_live_env".to_string()))
                .unwrap();
        assert!(active.uses_env_token());
        assert!(active.is_authenticated());
        assert_eq!(active.token().as_deref(), Some("ak_live_env"));
    }

    #[test]
    fn a_stored_key_is_saved_and_seen_by_every_clone() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        write_config(&path);

        let active = ActiveProfile::from_file(path.clone(), Some("staging"), None).unwrap();
        let shared_with_a_client = active.clone();
        active.store_api_key("ak_live_new".to_string()).unwrap();

        assert_eq!(shared_with_a_client.token().as_deref(), Some("ak_live_new"));
        assert_eq!(
            saved_profile(&path, "staging").api_key.as_deref(),
            Some("ak_live_new")
        );
    }

    #[test]
    fn a_change_picks_up_edits_made_by_another_process() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        write_config(&path);
        let active = ActiveProfile::from_file(path.clone(), None, None).unwrap();

        crate::config::update_profile_at(&path, "default", |profile| {
            profile.api_key = Some("ak_live_from_another_login".to_string())
        })
        .unwrap();
        active.clear_project().unwrap();

        assert_eq!(
            active.token().as_deref(),
            Some("ak_live_from_another_login")
        );
        assert_eq!(saved_profile(&path, "default").current_project_id, None);
    }

    #[test]
    fn clearing_the_project_reports_whether_one_was_selected() {
        let active = ActiveProfile::in_memory("default", Profile::default(), None);
        assert!(!active.clear_project().unwrap());

        active
            .select_project(Uuid::parse_str(PROJECT_ID).unwrap())
            .unwrap();
        assert!(active.clear_project().unwrap());
        assert_eq!(active.current_project_id(), None);
    }

    #[test]
    fn a_malformed_project_id_counts_as_none() {
        let profile = Profile {
            current_project_id: Some("not-a-uuid".to_string()),
            ..Profile::default()
        };
        let active = ActiveProfile::in_memory("default", profile, None);
        assert_eq!(active.current_project_id(), None);
    }

    #[test]
    fn the_api_url_is_saved_without_a_trailing_slash() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let active = ActiveProfile::from_file(path.clone(), None, None).unwrap();

        active.set_api_url("https://api.example.com/").unwrap();
        assert_eq!(active.api_url(), "https://api.example.com");
        assert_eq!(
            saved_profile(&path, "default").api_url,
            "https://api.example.com"
        );
    }

    #[test]
    fn forgetting_the_key_keeps_the_rest_of_the_profile() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        write_config(&path);
        let active = ActiveProfile::from_file(path.clone(), None, None).unwrap();

        active.forget_api_key().unwrap();
        let saved = saved_profile(&path, "default");
        assert_eq!(saved.api_key, None);
        assert_eq!(saved.current_project_id.as_deref(), Some(PROJECT_ID));
    }
}
