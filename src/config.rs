//! Loads and saves configuration. Never runs commands.
//!
//! Global: `$XDG_CONFIG_HOME/paddock/config.toml`, falling back to
//! `~/.config/paddock/config.toml` on macOS and Linux alike, which is where
//! terminal tools are expected to keep config. Holds the list of project
//! folders, UI preferences and key overrides. A missing file means defaults.
//!
//! Per project (to come with `detect`): optional `paddock.toml` in the
//! project root. Its processes replace detected ones with the same name.
//!
//! Unknown keys are ignored, so an older Paddock can read a newer config.
//! Saving writes a temp file and renames it, so a crash never leaves half a
//! config behind. Two parts of Paddock write the file (the TUI saves the UI
//! settings, the supervisor saves projects and process overrides), so every
//! write goes through `update`: read the file, change one part, write it
//! back, all under one lock.
//!
//! ```toml
//! [ui]
//! theme = "paddock"
//! splash = true
//!
//! [keys]
//! start = "s"
//! quit = ["q", "ctrl+q"]
//!
//! # Per process, keyed by "<project folder>#<process name>".
//! [processes."~/Projects/shop#web"]
//! port = 3100
//! group = "frontends"
//! ```

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub projects: Vec<PathBuf>,
    pub ui: Ui,
    /// Action name to one key or a list of keys, e.g. `start = "s"`.
    /// Actions not listed keep their default keys.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub keys: BTreeMap<String, Keys>,
    /// Changes the user made to detected processes.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub processes: BTreeMap<String, ProcessOverride>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProcessOverride {
    /// Port to run on instead of the detected one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    /// Project to show the process under instead of its own folder.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
}

impl ProcessOverride {
    pub fn is_empty(&self) -> bool {
        self.port.is_none() && self.group.is_none()
    }
}

/// The key for `Config::processes`.
pub fn process_key(project: &Path, name: &str) -> String {
    format!("{}#{name}", project.display())
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
    #[error("cannot find a config folder: neither XDG_CONFIG_HOME nor HOME is set")]
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
        .or_else(|| std::env::var_os("HOME").map(|home| Path::new(&home).join(".config")))
        .ok_or(ConfigError::NoConfigDir)?;
    Ok(base.join("paddock").join("config.toml"))
}

pub fn load() -> Result<Config, ConfigError> {
    load_from(&path()?)
}

pub fn save(config: &Config) -> Result<(), ConfigError> {
    save_to(config, &path()?)
}

static WRITE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Reads the current file, applies `change`, writes it back. Returns the
/// config as saved.
pub fn update(change: impl FnOnce(&mut Config)) -> Result<Config, ConfigError> {
    update_at(&path()?, change)
}

pub fn update_at(path: &Path, change: impl FnOnce(&mut Config)) -> Result<Config, ConfigError> {
    let _guard = WRITE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut config = load_from(path)?;
    change(&mut config);
    save_to(&config, path)?;
    Ok(config)
}

/// `~/x` to `$HOME/x`. Other paths are returned as they are.
pub fn expand_home(path: &Path) -> PathBuf {
    match (path.strip_prefix("~"), std::env::var_os("HOME")) {
        (Ok(rest), Some(home)) => Path::new(&home).join(rest),
        _ => path.to_owned(),
    }
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
            .insert("start".into(), Keys::One("ctrl+s".into()));
        config
            .keys
            .insert("quit".into(), Keys::Many(vec!["q".into(), "ctrl+q".into()]));

        save_to(&config, &path).unwrap();
        assert_eq!(load_from(&path).unwrap(), config);
    }

    #[test]
    fn unknown_and_missing_keys_are_fine() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, "future_option = 1\n[ui]\ntheme = \"dracula\"\n").unwrap();
        let config = load_from(&path).unwrap();
        assert_eq!(config.ui.theme, "dracula");
        assert!(config.ui.splash);
    }

    #[test]
    fn update_changes_one_part_and_keeps_the_rest() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        update_at(&path, |c| c.ui.theme = "nord".into()).unwrap();
        update_at(&path, |c| {
            c.processes.insert(
                process_key(Path::new("/p/shop"), "web"),
                ProcessOverride {
                    port: Some(3100),
                    group: None,
                },
            );
        })
        .unwrap();
        let config = load_from(&path).unwrap();
        assert_eq!(config.ui.theme, "nord");
        assert_eq!(config.processes["/p/shop#web"].port, Some(3100));
        let text = fs::read_to_string(&path).unwrap();
        assert!(
            !text.contains("group"),
            "empty fields are not written:\n{text}"
        );
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
