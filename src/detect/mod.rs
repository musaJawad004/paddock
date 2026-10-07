//! Works out how to run a project by reading its files. Read-only: never
//! writes, never executes anything, never touches the network.
//!
//! `detect(dir)` asks each detector and merges the results:
//!
//! 1. package.json scripts (`node`), Cargo binaries (`rust`), compose
//!    services (`compose`), Makefile targets (`procfile`).
//! 2. A Procfile replaces all of those: it is the user saying exactly what
//!    to run.
//! 3. `paddock.toml` in the project root has the last word: its processes
//!    replace detected ones with the same name, add new ones, or hide one
//!    with `enabled = false`.
//!
//! Malformed files are skipped, never fatal. Duplicate names get a number.

pub mod compose;
pub mod node;
pub mod procfile;
pub mod rust;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::model::ProcessSpec;

pub fn detect(dir: &Path) -> Vec<ProcessSpec> {
    let mut specs = match procfile::detect_procfile(dir) {
        Some(procfile) => procfile,
        None => {
            let mut found = node::detect(dir);
            found.extend(rust::detect(dir));
            found.extend(compose::detect(dir));
            for target in procfile::detect_makefile(dir) {
                if !found.iter().any(|s| s.name == target.name) {
                    found.push(target);
                }
            }
            found
        }
    };
    apply_paddock_toml(dir, &mut specs);
    unique_names(&mut specs);
    specs
}

/// Files that make a folder a project, with the label shown for them.
const MARKERS: &[(&str, &str)] = &[
    ("paddock.toml", "paddock"),
    ("Procfile", "procfile"),
    ("package.json", "node"),
    ("Cargo.toml", "rust"),
    ("compose.yaml", "compose"),
    ("compose.yml", "compose"),
    ("docker-compose.yml", "compose"),
    ("docker-compose.yaml", "compose"),
    ("go.mod", "go"),
    ("pyproject.toml", "python"),
    ("Gemfile", "ruby"),
];

/// A short label if `dir` looks like a project ("node", "rust"...). Only
/// checks which files exist; reads nothing.
pub fn project_kind(dir: &Path) -> Option<&'static str> {
    MARKERS
        .iter()
        .find(|(file, _)| dir.join(file).is_file())
        .map(|(_, kind)| *kind)
}

/// The project a working directory belongs to: the outermost folder with a
/// project file, walking up from `start` but never to `stop` (usually the
/// home folder) or above it. Outermost, so a server started inside
/// `apps/web` of a monorepo belongs to the monorepo.
pub fn project_root(start: &Path, stop: &Path) -> Option<PathBuf> {
    let mut found = None;
    let mut dir = Some(start);
    while let Some(current) = dir {
        if current == stop || !current.starts_with(stop) {
            break;
        }
        if project_kind(current).is_some() {
            found = Some(current.to_owned());
        }
        dir = current.parent();
    }
    found
}

/// The name shown for a project: its folder name.
pub fn project_name(dir: &Path) -> String {
    dir.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| dir.display().to_string())
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct PaddockToml {
    process: BTreeMap<String, ProcessEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
struct ProcessEntry {
    cmd: Option<String>,
    cwd: Option<PathBuf>,
    port: Option<u16>,
    env: BTreeMap<String, String>,
    enabled: bool,
}

impl Default for ProcessEntry {
    fn default() -> Self {
        Self {
            cmd: None,
            cwd: None,
            port: None,
            env: BTreeMap::new(),
            enabled: true,
        }
    }
}

fn apply_paddock_toml(dir: &Path, specs: &mut Vec<ProcessSpec>) {
    let Ok(text) = fs::read_to_string(dir.join("paddock.toml")) else {
        return;
    };
    let Ok(file) = toml::from_str::<PaddockToml>(&text) else {
        return;
    };
    for (name, entry) in file.process {
        let existing = specs.iter().position(|s| s.name == name);
        if !entry.enabled {
            if let Some(i) = existing {
                specs.remove(i);
            }
            continue;
        }
        let spec = match existing {
            Some(i) => &mut specs[i],
            None => {
                let Some(cmd) = &entry.cmd else {
                    continue;
                };
                specs.push(ProcessSpec {
                    name: name.clone(),
                    command: cmd.clone(),
                    cwd: dir.to_owned(),
                    port: None,
                    port_args: None,
                    env: Vec::new(),
                    source: format!("paddock.toml process \"{name}\""),
                });
                let last = specs.len() - 1;
                &mut specs[last]
            }
        };
        if let Some(cmd) = entry.cmd {
            spec.command = cmd;
            spec.port_args = None;
            spec.source = format!("paddock.toml process \"{name}\"");
        }
        if let Some(cwd) = entry.cwd {
            spec.cwd = dir.join(cwd);
        }
        if entry.port.is_some() {
            spec.port = entry.port;
        }
        spec.env.extend(entry.env);
    }
}

fn unique_names(specs: &mut [ProcessSpec]) {
    let mut seen: Vec<String> = Vec::new();
    for spec in specs.iter_mut() {
        let base = spec.name.clone();
        let mut n = 2;
        while seen.contains(&spec.name) {
            spec.name = format!("{base}-{n}");
            n += 1;
        }
        seen.push(spec.name.clone());
    }
}

/// `packages/*` style patterns (the only kind workspaces use in practice)
/// and plain folder names, sorted, folders only.
pub(crate) fn expand_patterns(dir: &Path, patterns: &[String]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for pattern in patterns {
        let pattern = pattern.trim_end_matches('/');
        if let Some(parent) = pattern.strip_suffix("/*") {
            if let Ok(entries) = fs::read_dir(dir.join(parent)) {
                out.extend(entries.flatten().map(|e| e.path()).filter(|p| p.is_dir()));
            }
        } else if !pattern.contains('*') && !pattern.starts_with('!') {
            let path = dir.join(pattern);
            if path.is_dir() {
                out.push(path);
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name)
    }

    fn names(specs: &[ProcessSpec]) -> Vec<String> {
        specs.iter().map(|s| s.name.clone()).collect()
    }

    #[test]
    fn a_procfile_replaces_package_json() {
        let specs = detect(&fixture("procfile"));
        assert_eq!(names(&specs), vec!["web", "worker"]);
        assert_eq!(specs[0].command, "yarn dev");
    }

    #[test]
    fn paddock_toml_overrides_adds_and_hides() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"scripts":{"dev":"vite","dev:docs":"vitepress dev"}}"#,
        )
        .unwrap();
        fs::write(
            dir.path().join("paddock.toml"),
            r#"
[process.dev]
port = 5000
env = { DEBUG = "1" }

[process."dev:docs"]
enabled = false

[process.mail]
cmd = "mailpit"
port = 8025
"#,
        )
        .unwrap();
        let specs = detect(dir.path());
        assert_eq!(names(&specs), vec!["dev", "mail"]);
        assert_eq!(specs[0].port, Some(5000));
        assert_eq!(specs[0].env, vec![("DEBUG".into(), "1".into())]);
        assert_eq!(specs[1].command, "mailpit");
    }

    #[test]
    fn duplicate_names_get_numbers() {
        let spec = |name: &str| ProcessSpec {
            name: name.into(),
            command: String::new(),
            cwd: PathBuf::new(),
            port: None,
            port_args: None,
            env: Vec::new(),
            source: String::new(),
        };
        let mut specs = vec![spec("dev"), spec("dev"), spec("dev")];
        unique_names(&mut specs);
        assert_eq!(names(&specs), vec!["dev", "dev-2", "dev-3"]);
    }

    #[test]
    fn project_root_is_the_outermost_project_below_home() {
        let home = tempfile::tempdir().unwrap();
        let repo = home.path().join("repo");
        let web = repo.join("apps/web");
        fs::create_dir_all(&web).unwrap();
        fs::write(repo.join("package.json"), "{}").unwrap();
        fs::write(web.join("package.json"), "{}").unwrap();
        assert_eq!(project_root(&web, home.path()), Some(repo.clone()));
        assert_eq!(project_kind(&repo), Some("node"));

        let loose = home.path().join("notes");
        fs::create_dir_all(&loose).unwrap();
        assert_eq!(project_root(&loose, home.path()), None);
        assert_eq!(project_root(Path::new("/"), home.path()), None);
    }

    #[test]
    fn an_empty_folder_has_nothing_to_run() {
        let dir = tempfile::tempdir().unwrap();
        assert!(detect(dir.path()).is_empty());
    }
}
