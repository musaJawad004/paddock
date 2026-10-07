//! `Procfile` (one `name: command` per line, as used by foreman and
//! overmind), and `Makefile` targets named `dev`, `run`, `serve` or `start`.
//!
//! A Procfile is the user's explicit list, so `detect` uses it instead of
//! anything else it finds.

use std::fs;
use std::path::Path;

use crate::model::ProcessSpec;

const MAKE_TARGETS: &[&str] = &["dev", "run", "serve", "start"];

pub fn detect_procfile(dir: &Path) -> Option<Vec<ProcessSpec>> {
    let text = fs::read_to_string(dir.join("Procfile")).ok()?;
    let specs = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| {
            let (name, command) = line.split_once(':')?;
            let (name, command) = (name.trim(), command.trim());
            if name.is_empty() || command.is_empty() || name.contains(' ') {
                return None;
            }
            Some(ProcessSpec {
                name: name.to_owned(),
                command: command.to_owned(),
                cwd: dir.to_owned(),
                port: None,
                port_args: None,
                env: Vec::new(),
                source: format!("Procfile \"{name}\""),
            })
        })
        .collect();
    Some(specs)
}

pub fn detect_makefile(dir: &Path) -> Vec<ProcessSpec> {
    let Ok(text) = fs::read_to_string(dir.join("Makefile")) else {
        return Vec::new();
    };
    MAKE_TARGETS
        .iter()
        .filter(|target| {
            text.lines().any(|line| {
                line.strip_prefix(**target)
                    .is_some_and(|rest| rest.starts_with(':'))
            })
        })
        .map(|target| ProcessSpec {
            name: (*target).to_owned(),
            command: format!("make {target}"),
            cwd: dir.to_owned(),
            port: None,
            port_args: None,
            env: Vec::new(),
            source: format!("Makefile target \"{target}\""),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn procfile_lines() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Procfile"),
            "# comment\nweb: bundle exec rails s -p $PORT\nbad line\nworker:   sidekiq\n",
        )
        .unwrap();
        let specs = detect_procfile(dir.path()).unwrap();
        let names: Vec<&str> = specs.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["web", "worker"]);
        assert_eq!(specs[1].command, "sidekiq");
    }

    #[test]
    fn makefile_targets() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Makefile"),
            "build:\n\tcargo build\ndev: build\n\tcargo run\nrunner:\n\techo no\n",
        )
        .unwrap();
        let specs = detect_makefile(dir.path());
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].command, "make dev");
    }
}
