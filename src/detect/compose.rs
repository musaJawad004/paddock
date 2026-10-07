//! Docker Compose: `compose.yaml`, `compose.yml`, `docker-compose.yml`,
//! `docker-compose.yaml`.
//!
//! Each service becomes `docker compose up <service>` in the foreground, so
//! its logs stream like any other process and stopping it stops the
//! container. The first published host port is the port hint.
//!
//! Compose files are YAML. Paddock reads only the `services:` keys and their
//! `ports:` entries with a small line reader instead of pulling in a YAML
//! parser; anything it does not understand is skipped.

use std::fs;
use std::path::Path;

use crate::model::ProcessSpec;

const FILES: &[&str] = &[
    "compose.yaml",
    "compose.yml",
    "docker-compose.yml",
    "docker-compose.yaml",
];

pub fn detect(dir: &Path) -> Vec<ProcessSpec> {
    let Some((file, text)) = FILES
        .iter()
        .find_map(|f| Some((*f, fs::read_to_string(dir.join(f)).ok()?)))
    else {
        return Vec::new();
    };
    services(&text)
        .into_iter()
        .map(|(name, port)| ProcessSpec {
            command: format!("docker compose up {name}"),
            cwd: dir.to_owned(),
            port,
            port_args: None,
            env: Vec::new(),
            source: format!("{file} service \"{name}\""),
            name,
        })
        .collect()
}

fn indent(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// Service names in order, each with its first published host port.
fn services(text: &str) -> Vec<(String, Option<u16>)> {
    let mut out: Vec<(String, Option<u16>)> = Vec::new();
    let mut in_services = false;
    let mut service_indent = None;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let depth = indent(line);
        if depth == 0 {
            in_services = trimmed == "services:";
            continue;
        }
        if !in_services {
            continue;
        }
        let level = *service_indent.get_or_insert(depth);
        if depth == level {
            if let Some(name) = trimmed.strip_suffix(':') {
                out.push((name.trim_matches(['"', '\'']).to_owned(), None));
            }
        } else if let (Some(entry), Some(last)) = (trimmed.strip_prefix("- "), out.last_mut())
            && last.1.is_none()
        {
            last.1 = host_port(entry);
        }
    }
    out
}

/// `"5432:5432"`, `8080:80`, `127.0.0.1:5432:5432`, `"3000:3000/tcp"`.
fn host_port(entry: &str) -> Option<u16> {
    let entry = entry.trim().trim_matches(['"', '\'']);
    let entry = entry.split('/').next()?;
    let parts: Vec<&str> = entry.split(':').collect();
    if parts.len() < 2 {
        return None;
    }
    parts[parts.len() - 2].parse().ok()
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn services_and_ports_from_the_fixture() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/compose");
        let specs = detect(&dir);
        let summary: Vec<(String, Option<u16>)> =
            specs.iter().map(|s| (s.name.clone(), s.port)).collect();
        assert_eq!(
            summary,
            vec![("db".into(), Some(5432)), ("cache".into(), None)]
        );
        assert_eq!(specs[0].command, "docker compose up db");
    }

    #[test]
    fn port_forms() {
        assert_eq!(host_port("\"5432:5432\""), Some(5432));
        assert_eq!(host_port("8080:80"), Some(8080));
        assert_eq!(host_port("127.0.0.1:6000:6379"), Some(6000));
        assert_eq!(host_port("3000:3000/tcp"), Some(3000));
        assert_eq!(host_port("9000"), None);
    }

    #[test]
    fn other_top_level_keys_are_ignored() {
        let text = "volumes:\n  data:\nservices:\n  web:\n    ports:\n      - 80:80\nnetworks:\n  front:\n";
        assert_eq!(services(text), vec![("web".to_string(), Some(80))]);
    }
}
