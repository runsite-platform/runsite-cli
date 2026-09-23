mod types;
pub use types::{Config, Profile};

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

fn config_path() -> Result<PathBuf> {
    let dir = dirs::config_dir()
        .context("cannot determine config directory")?
        .join("runsite");
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("config.toml"))
}

fn read_from(path: &Path) -> Result<Config> {
    if !path.exists() {
        let mut config = Config {
            current_profile: "default".to_string(),
            ..Default::default()
        };
        config
            .profiles
            .insert("default".to_string(), Profile::default());
        return Ok(config);
    }

    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    toml::from_str(&content).context("failed to parse config.toml")
}

fn write_to(path: &Path, config: &Config) -> Result<()> {
    let content = toml::to_string_pretty(config).context("failed to serialize config")?;
    // Write-then-rename, so a concurrent reader never sees a half-written file.
    // The process id keeps two runsite processes off each other's staging file.
    let staging = path.with_extension(format!("toml.{}.tmp", std::process::id()));
    std::fs::write(&staging, content)
        .with_context(|| format!("failed to write {}", staging.display()))?;
    // The file holds API keys: keep the permissions the user gave it.
    if let Ok(existing) = std::fs::metadata(path) {
        std::fs::set_permissions(&staging, existing.permissions())
            .with_context(|| format!("failed to write {}", staging.display()))?;
    }
    std::fs::rename(&staging, path)
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

pub fn load(profile_override: Option<&str>) -> Result<(Config, String)> {
    let mut config = read_from(&config_path()?)?;

    let active = profile_override
        .map(|s| s.to_string())
        .unwrap_or_else(|| config.current_profile.clone());

    if active != config.current_profile {
        config.current_profile = active.clone();
    }

    if !config.profiles.contains_key(&config.current_profile) {
        config
            .profiles
            .insert(config.current_profile.clone(), Profile::default());
    }

    Ok((config, active))
}

/// The config exactly as it is on disk right now, without any `--profile` override.
pub fn read_current() -> Result<Config> {
    read_from(&config_path()?)
}

pub fn save(config: &Config) -> Result<()> {
    let path = config_path()?;
    let content = toml::to_string_pretty(config).context("failed to serialize config")?;
    std::fs::write(&path, content)
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

/// Change one profile without touching anything else in the file.
///
/// The file is re-read first, so edits made meanwhile by another `runsite`
/// process (a login, `project use`) survive. `current_profile` is never changed.
pub fn update_profile(name: &str, change: impl FnOnce(&mut Profile)) -> Result<()> {
    update_profile_at(&config_path()?, name, change)
}

fn update_profile_at(path: &Path, name: &str, change: impl FnOnce(&mut Profile)) -> Result<()> {
    let mut config = read_from(path)?;
    change(config.profiles.entry(name.to_string()).or_default());
    write_to(path, &config)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_raw(path: &Path, content: &str) {
        std::fs::write(path, content).unwrap();
    }

    #[test]
    fn an_update_keeps_changes_made_by_another_process() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        write_raw(
            &path,
            r#"
current_profile = "default"

[profiles.default]
api_url = "https://api.runsite.app"
api_key = "ak_live_old"

[profiles.staging]
api_url = "https://staging.runsite.app"
"#,
        );

        // Another terminal logs in to staging after this process started.
        let mut on_disk = read_from(&path).unwrap();
        on_disk.profiles.get_mut("staging").unwrap().api_key = Some("ak_live_staging".into());
        write_to(&path, &on_disk).unwrap();

        update_profile_at(&path, "default", |profile| {
            profile.current_project_id = Some("ca83c4a9-1517-4fe3-93aa-bbdab4eaee23".into());
        })
        .unwrap();

        let result = read_from(&path).unwrap();
        assert_eq!(result.current_profile, "default");
        assert_eq!(
            result.profiles["staging"].api_key.as_deref(),
            Some("ak_live_staging")
        );
        assert_eq!(
            result.profiles["default"].api_key.as_deref(),
            Some("ak_live_old")
        );
        assert_eq!(
            result.profiles["default"].current_project_id.as_deref(),
            Some("ca83c4a9-1517-4fe3-93aa-bbdab4eaee23")
        );
    }

    #[cfg(unix)]
    #[test]
    fn an_update_keeps_the_file_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        write_raw(&path, "current_profile = \"default\"\n[profiles]\n");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();

        update_profile_at(&path, "default", |profile| {
            profile.api_key = Some("k".into())
        })
        .unwrap();

        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn an_update_creates_a_missing_file_and_profile() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");

        update_profile_at(&path, "work", |profile| {
            profile.api_key = Some("ak_live_new".into())
        })
        .unwrap();

        let result = read_from(&path).unwrap();
        assert_eq!(result.current_profile, "default");
        assert!(result.profiles.contains_key("default"));
        assert_eq!(
            result.profiles["work"].api_key.as_deref(),
            Some("ak_live_new")
        );
        assert_eq!(result.profiles["work"].api_url, "https://api.runsite.app");
    }
}
