use std::{fs, path::PathBuf};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::{
    error::{AppError, Result},
    APP_NAME,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Config {
    pub selected_property: Option<String>,
    pub selected_property_name: Option<String>,
    pub default_range_days: u16,
    pub comparison_enabled: bool,
    pub cache_enabled: bool,
    pub no_color: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            selected_property: None,
            selected_property_name: None,
            default_range_days: 28,
            comparison_enabled: true,
            cache_enabled: true,
            no_color: std::env::var_os("NO_COLOR").is_some(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Paths {
    pub config_dir: PathBuf,
    pub config_file: PathBuf,
    pub oauth_client_file: PathBuf,
    pub cache_dir: PathBuf,
}

impl Paths {
    pub fn discover() -> Result<Self> {
        let dirs = ProjectDirs::from("org", "omarchy", APP_NAME)
            .ok_or_else(|| AppError::Config("cannot resolve XDG directories".into()))?;
        Ok(Self::from_dirs(
            dirs.config_dir().to_path_buf(),
            dirs.cache_dir().to_path_buf(),
        ))
    }

    pub fn from_dirs(config_dir: PathBuf, cache_dir: PathBuf) -> Self {
        Self {
            config_file: config_dir.join("config.toml"),
            oauth_client_file: config_dir.join("oauth-client.json"),
            config_dir,
            cache_dir: cache_dir.join("reports"),
        }
    }
}

impl Config {
    pub fn load(paths: &Paths) -> Result<Self> {
        if !paths.config_file.exists() {
            return Ok(Self::default());
        }
        let contents = fs::read_to_string(&paths.config_file)
            .map_err(|error| AppError::Config(error.to_string()))?;
        toml::from_str(&contents).map_err(|error| AppError::Config(error.to_string()))
    }

    pub fn save(&self, paths: &Paths) -> Result<()> {
        fs::create_dir_all(&paths.config_dir)
            .map_err(|error| AppError::Config(error.to_string()))?;
        let data =
            toml::to_string_pretty(self).map_err(|error| AppError::Config(error.to_string()))?;
        fs::write(&paths.config_file, data).map_err(|error| AppError::Config(error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn config_round_trips_without_credentials() {
        let root = tempdir().unwrap();
        let paths = Paths::from_dirs(root.path().join("config"), root.path().join("cache"));
        let config = Config {
            selected_property: Some("123".into()),
            ..Config::default()
        };
        config.save(&paths).unwrap();
        assert_eq!(Config::load(&paths).unwrap(), config);
        assert!(!fs::read_to_string(paths.config_file)
            .unwrap()
            .contains("token"));
    }
}
