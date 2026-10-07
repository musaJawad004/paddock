//! Rust projects: `Cargo.toml`.
//!
//! Each binary target becomes a process: `src/main.rs`, `src/bin/*.rs`
//! and `[[bin]]` entries. One binary runs as `cargo run`, several as
//! `cargo run --bin <name>`. In a workspace without a root package, each
//! member's binaries run with `-p <package>`. Library-only crates produce
//! nothing.

use std::fs;
use std::path::Path;

use toml::Value;

use crate::model::ProcessSpec;

pub fn detect(dir: &Path) -> Vec<ProcessSpec> {
    let Some(manifest) = read(dir) else {
        return Vec::new();
    };
    if manifest.get("package").is_some() {
        return binaries(dir, &manifest, None, dir);
    }
    let members: Vec<String> = manifest
        .get("workspace")
        .and_then(|w| w.get("members"))
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    super::expand_patterns(dir, &members)
        .into_iter()
        .filter_map(|member| {
            let manifest = read(&member)?;
            let package = manifest.get("package")?.get("name")?.as_str()?.to_owned();
            Some(binaries(&member, &manifest, Some(&package), dir))
        })
        .flatten()
        .collect()
}

fn read(dir: &Path) -> Option<Value> {
    let text = fs::read_to_string(dir.join("Cargo.toml")).ok()?;
    toml::from_str(&text).ok()
}

/// `run_from` is where `cargo run` runs: the workspace root for members.
fn binaries(
    dir: &Path,
    manifest: &Value,
    package: Option<&str>,
    run_from: &Path,
) -> Vec<ProcessSpec> {
    let package_name = manifest
        .get("package")
        .and_then(|p| p.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("app");
    let mut bins: Vec<String> = manifest
        .get("bin")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|b| b.get("name")?.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    if dir.join("src/main.rs").exists() && !bins.iter().any(|b| b == package_name) {
        bins.push(package_name.to_owned());
    }
    if let Ok(entries) = fs::read_dir(dir.join("src/bin")) {
        let mut found: Vec<String> = entries
            .flatten()
            .filter_map(|e| {
                let path = e.path();
                let name = if path.extension()? == "rs" {
                    path.file_stem()?.to_string_lossy().into_owned()
                } else if path.join("main.rs").exists() {
                    path.file_name()?.to_string_lossy().into_owned()
                } else {
                    return None;
                };
                Some(name)
            })
            .collect();
        found.sort();
        for name in found {
            if !bins.contains(&name) {
                bins.push(name);
            }
        }
    }

    let several = bins.len() > 1;
    bins.into_iter()
        .map(|bin| {
            let mut command = String::from("cargo run");
            if let Some(package) = package {
                command.push_str(&format!(" -p {package}"));
            }
            if several {
                command.push_str(&format!(" --bin {bin}"));
            }
            ProcessSpec {
                command,
                cwd: run_from.to_owned(),
                port: None,
                port_args: None,
                env: Vec::new(),
                source: format!("Cargo.toml binary \"{bin}\""),
                name: bin,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn one_process_per_binary() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rust-bins");
        let specs = detect(&dir);
        let summary: Vec<(String, String)> = specs
            .iter()
            .map(|s| (s.name.clone(), s.command.clone()))
            .collect();
        assert_eq!(
            summary,
            vec![
                ("rust-bins".into(), "cargo run --bin rust-bins".into()),
                ("worker".into(), "cargo run --bin worker".into()),
            ]
        );
    }

    #[test]
    fn workspace_members_run_from_the_root() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/*\"]\n",
        )
        .unwrap();
        fs::create_dir_all(root.join("crates/api/src")).unwrap();
        fs::write(
            root.join("crates/api/Cargo.toml"),
            "[package]\nname = \"api\"\n",
        )
        .unwrap();
        fs::write(root.join("crates/api/src/main.rs"), "fn main() {}\n").unwrap();
        fs::create_dir_all(root.join("crates/core/src")).unwrap();
        fs::write(
            root.join("crates/core/Cargo.toml"),
            "[package]\nname = \"core\"\n",
        )
        .unwrap();
        fs::write(root.join("crates/core/src/lib.rs"), "\n").unwrap();

        let specs = detect(root);
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].command, "cargo run -p api");
        assert_eq!(specs[0].cwd, root);
    }
}
