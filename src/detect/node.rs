//! Node projects: `package.json` scripts.
//!
//! - Package manager from `packageManager`, else the lockfile: `yarn.lock`,
//!   `pnpm-lock.yaml`, `bun.lock`/`bun.lockb`, otherwise npm.
//! - Long-running scripts become processes: `dev`, `dev:*`, `watch`,
//!   `serve`, and anything that runs a known dev server. `start` and
//!   `preview` count only when there is no `dev`, since they usually serve a
//!   production build of the same app. Build, test and lint scripts never.
//! - The port comes from a flag in the script (`--port 4000`, `-p 4000`,
//!   `PORT=4000`), else the tool's default.
//! - Workspaces (`workspaces` in package.json, `pnpm-workspace.yaml`): when
//!   the root has no dev script of its own, each member's become processes.

use std::fs;
use std::path::Path;

use serde_json::Value;

use crate::model::ProcessSpec;

/// Dev servers: what the script contains, default port, how to change it.
const SERVERS: &[(&str, Option<u16>, Option<&str>)] = &[
    ("next dev", Some(3000), Some("-p {port}")),
    ("next start", Some(3000), Some("-p {port}")),
    ("expo start", Some(8081), Some("--port {port}")),
    ("astro dev", Some(4321), Some("--port {port}")),
    ("nuxt dev", Some(3000), Some("--port {port}")),
    ("nuxi dev", Some(3000), Some("--port {port}")),
    ("ng serve", Some(4200), Some("--port {port}")),
    ("react-scripts start", Some(3000), None),
    ("storybook dev", Some(6006), Some("--port {port}")),
    ("start-storybook", Some(6006), Some("--port {port}")),
    ("webpack serve", Some(8080), Some("--port {port}")),
    ("gatsby develop", Some(8000), Some("--port {port}")),
    ("docusaurus start", Some(3000), Some("--port {port}")),
    ("remix dev", Some(3000), None),
    ("vite", Some(5173), Some("--port {port}")),
    ("nodemon", None, None),
    ("tsx watch", None, None),
    ("ts-node-dev", None, None),
    ("node --watch", None, None),
    ("wrangler dev", Some(8787), Some("--port {port}")),
];

const NEVER: &[&str] = &[
    "build",
    "test",
    "lint",
    "format",
    "typecheck",
    "clean",
    "prepare",
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Manager {
    Npm,
    Yarn,
    Pnpm,
    Bun,
}

impl Manager {
    fn run(self, script: &str) -> String {
        match self {
            Manager::Npm => format!("npm run {script}"),
            Manager::Yarn => format!("yarn {script}"),
            Manager::Pnpm => format!("pnpm run {script}"),
            Manager::Bun => format!("bun run {script}"),
        }
    }

    /// npm needs `--` before arguments meant for the script.
    fn pass_args(self, args: &str) -> String {
        match self {
            Manager::Npm => format!("-- {args}"),
            _ => args.to_owned(),
        }
    }
}

pub fn detect(dir: &Path) -> Vec<ProcessSpec> {
    let Some(root) = read_json(&dir.join("package.json")) else {
        return Vec::new();
    };
    let manager = manager(dir, &root);
    let own = scripts(dir, &root, manager, None);
    if !own.is_empty() {
        return own;
    }
    workspace_members(dir, &root)
        .into_iter()
        .flat_map(|member| {
            let json = read_json(&member.join("package.json"))?;
            let name = member.file_name()?.to_string_lossy().into_owned();
            Some(scripts(&member, &json, manager, Some(&name)))
        })
        .flatten()
        .collect()
}

fn read_json(path: &Path) -> Option<Value> {
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

fn manager(dir: &Path, package: &Value) -> Manager {
    let declared = package
        .get("packageManager")
        .and_then(Value::as_str)
        .unwrap_or_default();
    for (prefix, manager) in [
        ("pnpm", Manager::Pnpm),
        ("yarn", Manager::Yarn),
        ("bun", Manager::Bun),
        ("npm", Manager::Npm),
    ] {
        if declared.starts_with(prefix) {
            return manager;
        }
    }
    let has = |file: &str| dir.join(file).exists();
    if has("pnpm-lock.yaml") {
        Manager::Pnpm
    } else if has("yarn.lock") {
        Manager::Yarn
    } else if has("bun.lock") || has("bun.lockb") {
        Manager::Bun
    } else {
        Manager::Npm
    }
}

fn scripts(
    dir: &Path,
    package: &Value,
    manager: Manager,
    member: Option<&str>,
) -> Vec<ProcessSpec> {
    let Some(scripts) = package.get("scripts").and_then(Value::as_object) else {
        return Vec::new();
    };
    let has_dev = scripts.contains_key("dev");
    let mut chosen: Vec<(&String, &str)> = scripts
        .iter()
        .filter_map(|(name, command)| Some((name, command.as_str()?)))
        .filter(|(name, command)| is_long_running(name, command, has_dev))
        .collect();
    chosen.sort_by_key(|(name, _)| script_rank(name));

    let several = chosen.len() > 1;
    chosen
        .into_iter()
        .map(|(script, command)| {
            let (port, port_args) = port_of(command);
            let name = match member {
                Some(member) if several => format!("{member}:{script}"),
                Some(member) => member.to_owned(),
                None => script.clone(),
            };
            ProcessSpec {
                name,
                command: manager.run(script),
                cwd: dir.to_owned(),
                port,
                port_args: port_args.map(|args| manager.pass_args(args)),
                env: Vec::new(),
                source: format!("package.json script \"{script}\""),
            }
        })
        .collect()
}

fn is_long_running(name: &str, command: &str, has_dev: bool) -> bool {
    let base = name.split(':').next().unwrap_or(name);
    if NEVER.contains(&base) || name.starts_with("pre") || name.starts_with("post") {
        return false;
    }
    match base {
        "dev" | "watch" | "serve" | "develop" => true,
        "start" | "preview" => !has_dev,
        _ => SERVERS.iter().any(|(tool, ..)| command.contains(tool)) && !has_dev,
    }
}

/// `dev` first, then the rest by name, so the main process heads the list.
fn script_rank(name: &str) -> (u8, String) {
    let rank = match name {
        "dev" => 0,
        "start" => 1,
        _ => 2,
    };
    (rank, name.to_owned())
}

fn port_of(command: &str) -> (Option<u16>, Option<&'static str>) {
    let tool = SERVERS.iter().find(|(tool, ..)| command.contains(tool));
    let port_args = tool.and_then(|(_, _, args)| *args);
    (
        explicit_port(command).or(tool.and_then(|(_, port, _)| *port)),
        port_args,
    )
}

/// `--port 4000`, `--port=4000`, `-p 4000`, `PORT=4000`.
fn explicit_port(command: &str) -> Option<u16> {
    let words: Vec<&str> = command.split_whitespace().collect();
    for (i, word) in words.iter().enumerate() {
        let value = if let Some(v) = word
            .strip_prefix("--port=")
            .or_else(|| word.strip_prefix("PORT="))
        {
            Some(v)
        } else if *word == "--port" || *word == "-p" {
            words.get(i + 1).copied()
        } else {
            None
        };
        if let Some(port) = value.and_then(|v| v.parse().ok()) {
            return Some(port);
        }
    }
    None
}

fn workspace_members(dir: &Path, package: &Value) -> Vec<std::path::PathBuf> {
    let mut patterns: Vec<String> = match package.get("workspaces") {
        Some(Value::Array(list)) => list
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect(),
        Some(Value::Object(map)) => map
            .get("packages")
            .and_then(Value::as_array)
            .map(|list| {
                list.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
        _ => Vec::new(),
    };
    if let Ok(text) = fs::read_to_string(dir.join("pnpm-workspace.yaml")) {
        patterns.extend(
            text.lines()
                .filter_map(|line| line.trim().strip_prefix("- "))
                .map(|p| p.trim().trim_matches(['"', '\'']).to_owned()),
        );
    }
    super::expand_patterns(dir, &patterns)
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    fn fixture(name: &str) -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name)
    }

    fn summary(specs: &[ProcessSpec]) -> Vec<(String, String, Option<u16>)> {
        specs
            .iter()
            .map(|s| (s.name.clone(), s.command.clone(), s.port))
            .collect()
    }

    #[test]
    fn vite_with_yarn() {
        let specs = detect(&fixture("node-vite"));
        assert_eq!(
            summary(&specs),
            vec![("dev".into(), "yarn dev".into(), Some(5173))]
        );
        assert_eq!(specs[0].port_args.as_deref(), Some("--port {port}"));
    }

    #[test]
    fn pnpm_workspace_members_become_processes() {
        let specs = detect(&fixture("node-workspace"));
        assert_eq!(
            summary(&specs),
            vec![
                ("api".into(), "pnpm run start".into(), None),
                ("web".into(), "pnpm run dev".into(), Some(3000)),
            ]
        );
        assert_eq!(specs[1].port_args.as_deref(), Some("-p {port}"));
    }

    #[test]
    fn broken_package_json_is_skipped() {
        assert!(detect(&fixture("broken")).is_empty());
    }

    #[test]
    fn start_counts_only_without_dev() {
        assert!(is_long_running("start", "expo start", false));
        assert!(!is_long_running("start", "next start", true));
        assert!(!is_long_running("build", "vite build", false));
        assert!(!is_long_running("predev", "node setup.js", false));
    }

    #[test]
    fn explicit_ports_win() {
        assert_eq!(explicit_port("vite --port 4000"), Some(4000));
        assert_eq!(explicit_port("PORT=3001 node server.js"), Some(3001));
        assert_eq!(explicit_port("next dev -p 3005"), Some(3005));
        assert_eq!(explicit_port("vite"), None);
    }

    #[test]
    fn npm_needs_a_double_dash_for_script_arguments() {
        assert_eq!(Manager::Npm.pass_args("--port {port}"), "-- --port {port}");
        assert_eq!(Manager::Yarn.pass_args("--port {port}"), "--port {port}");
    }
}
