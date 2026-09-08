mod types;
pub use types::{Config, Profile};

use anyhow::{Context, Result};
use std::path::PathBuf;

fn config_path() -> Result<PathBuf> {
    let dir = dirs::config_dir()
        .context("cannot determine config directory")?
        .join("runsite");
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("config.toml"))
}

pub fn load(profile_override: Option<&str>) -> Result<(Config, String)> {
    let path = config_path()?;

    let mut config: Config = if path.exists() {
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        toml::from_str(&content).context("failed to parse config.toml")?
    } else {
        let mut c = Config {
            current_profile: "default".to_string(),
            ..Default::default()
        };
        c.profiles.insert("default".to_string(), Profile::default());
        c
    };

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

pub fn save(config: &Config) -> Result<()> {
    let path = config_path()?;
    let content = toml::to_string_pretty(config).context("failed to serialize config")?;
    std::fs::write(&path, content)
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}
