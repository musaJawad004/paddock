//! Loads and saves the user's settings. Never runs commands.
//!
//! `$XDG_CONFIG_HOME/paddock/config.toml`, falling back to
//! `~/.config/paddock/config.toml` on macOS and Linux alike, which is where
//! terminal tools are expected to keep config. A missing file means
//! defaults. Unknown keys are ignored, so an older Paddock can read a newer
//! config. Saving writes a temp file and renames it, so a crash never leaves
//! half a config behind.
//!
//! ```toml
//! [ui]
//! theme = "paddock"
//! splash = true
//!
//! [keys]
//! stop = "x"
//! quit = ["q", "ctrl+q"]
//! ```

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub ui: Ui,
    /// Action name to one key or a list of keys, e.g. `stop = "x"`.
    /// Actions not listed keep their default keys.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub keys: BTreeMap<String, Keys>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Ui {
    pub theme: String,
    pub splash: bool,
}

impl Default for Ui {
    fn default() -> Self {
        Self {
            theme: "paddock".into(),
            splash: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Keys {
    One(String),
    Many(Vec<String>),
}

impl Keys {
    pub fn list(&self) -> Vec<String> {
        match self {
            Self::One(key) => vec![key.clone()],
            Self::Many(keys) => keys.clone(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("cannot find a config folder: XDG_CONFIG_HOME, HOME and USERPROFILE are all unset")]
    NoConfigDir,
    #[error("cannot read {path}: {source}")]
    Read { path: PathBuf, source: io::Error },
    #[error("{path} is not valid TOML: {source}")]
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
    #[error("cannot write {path}: {source}")]
    Write { path: PathBuf, source: io::Error },
    #[error("cannot serialize the config: {0}")]
    Serialize(#[from] toml::ser::Error),
}

pub fn path() -> Result<PathBuf, ConfigError> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .or_else(|| std::env::var_os("USERPROFILE"))
                .map(|home| Path::new(&home).join(".config"))
        })
        .ok_or(ConfigError::NoConfigDir)?;
    Ok(base.join("paddock").join("config.toml"))
}

pub fn load() -> Result<Config, ConfigError> {
    load_from(&path()?)
}

pub fn save(config: &Config) -> Result<(), ConfigError> {
    save_to(config, &path()?)
}

pub fn load_from(path: &Path) -> Result<Config, ConfigError> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(Config::default()),
        Err(source) => {
            return Err(ConfigError::Read {
                path: path.to_owned(),
                source,
            });
        }
    };
    toml::from_str(&text).map_err(|source| ConfigError::Parse {
        path: path.to_owned(),
        source,
    })
}

pub fn save_to(config: &Config, path: &Path) -> Result<(), ConfigError> {
    let text = toml::to_string_pretty(config)?;
    let write_error = |source| ConfigError::Write {
        path: path.to_owned(),
        source,
    };
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(write_error)?;
    }
    let temp = path.with_extension("toml.tmp");
    fs::write(&temp, text).map_err(write_error)?;
    fs::rename(&temp, path).map_err(write_error)
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn a_missing_file_gives_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let config = load_from(&dir.path().join("config.toml")).unwrap();
        assert_eq!(config, Config::default());
        assert_eq!(config.ui.theme, "paddock");
        assert!(config.ui.splash);
    }

    #[test]
    fn saves_and_loads_back() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("config.toml");
        let mut config = Config::default();
        config.ui.theme = "nord".into();
        config.ui.splash = false;
        config
            .keys
            .insert("stop".into(), Keys::One("ctrl+s".into()));
        config
            .keys
            .insert("quit".into(), Keys::Many(vec!["q".into(), "ctrl+q".into()]));

        save_to(&config, &path).unwrap();
        assert_eq!(load_from(&path).unwrap(), config);
    }

    #[test]
    fn unknown_and_old_keys_are_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(
            &path,
            "projects = [\"/old\"]\n[ui]\ntheme = \"dracula\"\n[processes.\"x#y\"]\nport = 1\n",
        )
        .unwrap();
        let config = load_from(&path).unwrap();
        assert_eq!(config.ui.theme, "dracula");
        assert!(config.ui.splash);
    }

    #[test]
    fn broken_toml_is_an_error_that_names_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, "[ui\n").unwrap();
        let err = load_from(&path).unwrap_err().to_string();
        assert!(err.contains("config.toml"), "{err}");
    }
}
