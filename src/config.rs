use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub profiles: ProfilesConfig,
    #[serde(default)]
    pub repositories: RepositoriesConfig,
}

#[derive(Debug, Default, Deserialize)]
pub struct ProfilesConfig {
    #[serde(default)]
    pub default: Option<String>,
    #[serde(default)]
    pub paths: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct RepositoriesConfig {
    #[serde(default)]
    pub pools: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
}

impl Config {
    /// Returns the resolved profile discovery paths, with `~` expanded.
    pub fn profile_paths(&self) -> Vec<PathBuf> {
        self.profiles
            .paths
            .iter()
            .map(|p| expand_tilde(p))
            .collect()
    }

    /// Returns the resolved repository pool paths, with `~` expanded.
    pub fn pool_paths(&self) -> Vec<PathBuf> {
        self.repositories
            .pools
            .iter()
            .map(|p| expand_tilde(p))
            .collect()
    }
}

/// Loads the yarm configuration from `~/.config/yarm.toml`.
/// Returns a default config if the file does not exist.
pub fn load() -> Result<Config> {
    let Some(config_path) = config_path() else {
        return Ok(Config::default());
    };

    if !config_path.exists() {
        return Ok(Config::default());
    }

    let content =
        fs::read_to_string(&config_path).context("Failed to read yarm configuration file")?;

    toml::from_str(&content).context("Failed to parse yarm configuration file")
}

/// Returns the path to the yarm configuration file.
fn config_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".config/yarm.toml"))
}

/// Expands a leading `~/` to the user's home directory.
pub fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_config() {
        let config: Config = toml::from_str("").unwrap();
        assert!(config.profiles.default.is_none());
        assert!(config.profiles.paths.is_empty());
        assert!(config.repositories.pools.is_empty());
    }

    #[test]
    fn test_config_with_paths() {
        let config: Config = toml::from_str(
            r#"
[profiles]
paths = ["/custom/path", "~/gitconfigs"]
"#,
        )
        .unwrap();
        assert_eq!(config.profiles.paths.len(), 2);
        assert_eq!(config.profiles.paths[0], "/custom/path");
        assert_eq!(config.profiles.paths[1], "~/gitconfigs");
    }

    #[test]
    fn test_profile_paths_expansion() {
        let config: Config = toml::from_str(
            r#"
[profiles]
paths = ["/absolute/path"]
"#,
        )
        .unwrap();
        let paths = config.profile_paths();
        assert_eq!(paths[0], PathBuf::from("/absolute/path"));
    }

    #[test]
    fn test_config_with_default() {
        let config: Config = toml::from_str(
            r#"
[profiles]
default = "work"
"#,
        )
        .unwrap();
        assert_eq!(config.profiles.default.as_deref(), Some("work"));
    }

    #[test]
    fn test_config_with_pools() {
        let config: Config = toml::from_str(
            r#"
[repositories]
pools = ["~/projects", "/work/repos"]
"#,
        )
        .unwrap();
        assert_eq!(config.repositories.pools.len(), 2);
        let paths = config.pool_paths();
        assert_eq!(paths[1], PathBuf::from("/work/repos"));
    }

    #[test]
    fn test_expand_tilde_absolute() {
        assert_eq!(expand_tilde("/absolute/path"), PathBuf::from("/absolute/path"));
    }

    #[test]
    fn test_expand_tilde_with_home() {
        let expanded = expand_tilde("~/some/path");
        if let Some(home) = dirs::home_dir() {
            assert_eq!(expanded, home.join("some/path"));
        }
    }
}
