//! Which project a running server belongs to, from the folder it runs in.
//! Only checks which files exist; never reads their contents, never runs
//! anything.

use std::path::{Path, PathBuf};

/// Files that make a folder a project, with the label shown for them.
const MARKERS: &[(&str, &str)] = &[
    ("package.json", "node"),
    ("Cargo.toml", "rust"),
    ("go.mod", "go"),
    ("pyproject.toml", "python"),
    ("requirements.txt", "python"),
    ("Gemfile", "ruby"),
    ("composer.json", "php"),
    ("compose.yaml", "compose"),
    ("compose.yml", "compose"),
    ("docker-compose.yml", "compose"),
    ("docker-compose.yaml", "compose"),
    ("Procfile", "procfile"),
];

/// A short label if `dir` looks like a project ("node", "rust"...).
pub fn kind(dir: &Path) -> Option<&'static str> {
    MARKERS
        .iter()
        .find(|(file, _)| dir.join(file).is_file())
        .map(|(_, kind)| *kind)
}

/// The project a working directory belongs to: the outermost folder with a
/// project file, walking up from `start` but never to `stop` (the home
/// folder) or above it. Outermost, so a server started inside `apps/web` of
/// a monorepo belongs to the monorepo. Without any project file, the folder
/// itself.
pub fn root(start: &Path, stop: &Path) -> PathBuf {
    let mut found = None;
    let mut dir = Some(start);
    while let Some(current) = dir {
        if current == stop || !current.starts_with(stop) {
            break;
        }
        if kind(current).is_some() {
            found = Some(current);
        }
        dir = current.parent();
    }
    found.unwrap_or(start).to_owned()
}

/// The name shown for a project: its folder name.
pub fn name(dir: &Path) -> String {
    dir.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| dir.display().to_string())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn root_is_the_outermost_project_below_home() {
        let home = tempfile::tempdir().unwrap();
        let repo = home.path().join("repo");
        let web = repo.join("apps/web");
        fs::create_dir_all(&web).unwrap();
        fs::write(repo.join("package.json"), "{}").unwrap();
        fs::write(web.join("package.json"), "{}").unwrap();
        assert_eq!(root(&web, home.path()), repo);
        assert_eq!(kind(&repo), Some("node"));
        assert_eq!(name(&repo), "repo");
    }

    #[test]
    fn a_folder_without_project_files_is_its_own_project() {
        let home = tempfile::tempdir().unwrap();
        let notes = home.path().join("notes");
        fs::create_dir_all(&notes).unwrap();
        assert_eq!(root(&notes, home.path()), notes);
        assert_eq!(kind(&notes), None);
    }
}
