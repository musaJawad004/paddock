//! Turns "who listens on which port" plus the process table into projects
//! and servers. Pure: everything it needs comes through `ProcessTable`, so
//! it is tested with a made-up table.
//!
//! For each listener that belongs to the user and runs below their home
//! folder:
//!
//! 1. The project is the outermost folder with a project file around the
//!    listener's working directory (`project::root`).
//! 2. The server is the whole tree the user started: walk up the parents
//!    while they run in that project *and* are dev runners (npm, yarn,
//!    `sh -c`, cargo, make...). The walk never climbs into an interactive
//!    shell, an editor, a terminal or an agent such as Claude Code, so
//!    stopping a server can never take those down with it.
//!    `npm run dev` → `sh -c` → `node vite` is one server named "dev".
//!
//! Everything else on a dev-looking port (system apps, other users) is a
//! foreign port, shown so it can be freed.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use super::ports::Listener;
use crate::model::{
    ListeningPort, Project, ResourceUsage, Server, ServerId, ServerState, Snapshot,
};
use crate::project;

pub trait ProcessTable {
    fn parent(&self, pid: u32) -> Option<u32>;
    fn cwd(&self, pid: u32) -> Option<PathBuf>;
    /// Executable name, e.g. "node".
    fn name(&self, pid: u32) -> Option<String>;
    fn cmd(&self, pid: u32) -> Vec<String>;
    fn is_mine(&self, pid: u32) -> bool;
    /// Unix seconds.
    fn start_time(&self, pid: u32) -> Option<u64>;
    /// `root` and every process below it.
    fn tree(&self, root: u32) -> Vec<u32>;
    fn usage(&self, pids: &[u32]) -> ResourceUsage;
}

/// Foreign listeners are shown only on ports dev servers tend to use.
const DEV_PORTS: std::ops::RangeInclusive<u16> = 1024..=9999;

const SHELLS: &[&str] = &["zsh", "bash", "fish", "sh", "dash", "ksh", "tcsh", "csh"];

/// Programs that start dev servers and belong to them. Anything else (an
/// editor, a terminal, an agent, an interactive shell) ends the walk.
const RUNNERS: &[&str] = &[
    "npm",
    "npx",
    "yarn",
    "pnpm",
    "pnpx",
    "bun",
    "bunx",
    "deno",
    "cargo",
    "go",
    "make",
    "turbo",
    "nx",
    "concurrently",
    "nodemon",
    "tsx",
    "ts-node",
    "ts-node-dev",
    "uv",
    "poetry",
    "pipenv",
    "bundle",
    "foreman",
    "honcho",
    "watchexec",
    "air",
    "mix",
    "docker-compose",
];

/// Interpreters a server's own processes run in (`node vite`, `python -m
/// uvicorn`). The walk passes through them unless they run an agent.
const INTERPRETERS: &[&str] = &[
    "node", "deno", "bun", "python", "python3", "Python", "ruby", "php",
];

/// Coding agents and editors that run in an interpreter and start servers.
/// The walk stops below them, so stopping a server never stops them.
const AGENTS: &[&str] = &[
    "claude",
    "codex",
    "gemini",
    "opencode",
    "aider",
    "goose",
    "cursor-agent",
    "copilot",
    "code",
    "cursor",
    "amp",
    "crush",
];

/// Scripts that `node` runs on behalf of a package manager or runner.
const NODE_RUNNER_SCRIPTS: &[(&str, &str)] = &[
    ("yarn.js", "yarn"),
    ("yarn.cjs", "yarn"),
    ("npm-cli.js", "npm"),
    ("npx-cli.js", "npx"),
    ("pnpm.cjs", "pnpm"),
    ("pnpm.js", "pnpm"),
    ("turbo", "turbo"),
    ("nodemon", "nodemon"),
    ("concurrently", "concurrently"),
    ("tsx", "tsx"),
];

pub fn collect(
    listeners: &[Listener],
    table: &impl ProcessTable,
    home: &Path,
    stopping: &HashSet<ServerId>,
    protected: &HashSet<u32>,
) -> Snapshot {
    let mut servers: HashMap<ServerId, Server> = HashMap::new();
    let mut project_of: HashMap<ServerId, PathBuf> = HashMap::new();
    let mut ports = Vec::new();

    for listener in listeners {
        let owner = server_of(listener.pid, table, home, protected);
        if let Some((id, root_dir, named_by)) = &owner {
            let entry = servers.entry(*id).or_insert_with(|| {
                let tree: Vec<u32> = table
                    .tree(id.pid)
                    .into_iter()
                    .filter(|p| !protected.contains(p))
                    .collect();
                let cmd = clean_cmd(table.cmd(*named_by));
                Server {
                    id: *id,
                    name: server_name(&cmd, table.name(*named_by).as_deref()),
                    command: cmd.join(" "),
                    cwd: table.cwd(id.pid).unwrap_or_else(|| root_dir.clone()),
                    ports: Vec::new(),
                    processes: tree.len(),
                    usage: table.usage(&tree),
                    state: if stopping.contains(id) {
                        ServerState::Stopping
                    } else {
                        ServerState::Running
                    },
                }
            });
            if !entry.ports.contains(&listener.port) {
                entry.ports.push(listener.port);
            }
            project_of.insert(*id, root_dir.clone());
        } else if !DEV_PORTS.contains(&listener.port) {
            continue;
        }
        ports.push(ListeningPort {
            port: listener.port,
            pid: listener.pid,
            command: listener.command.clone(),
            owner: owner.map(|(id, _, _)| id),
        });
    }

    let mut grouped: BTreeMap<String, Project> = BTreeMap::new();
    for (id, mut server) in servers {
        server.ports.sort_unstable();
        let path = project_of.remove(&id).unwrap_or_default();
        let key = path.display().to_string();
        grouped
            .entry(key)
            .or_insert_with(|| Project {
                name: project::name(&path),
                kind: project::kind(&path).unwrap_or("folder").to_owned(),
                path,
                servers: Vec::new(),
            })
            .servers
            .push(server);
    }
    let mut projects: Vec<Project> = grouped.into_values().collect();
    for project in &mut projects {
        project.servers.sort_by_key(|s| s.ports.first().copied());
    }
    projects.sort_by_key(|p| p.name.to_lowercase());
    // One row per port and server: a reloading server can have several
    // processes on the same port.
    ports.sort_by_key(|p| (p.port, p.owner.map(|o| o.pid), p.pid));
    ports.dedup_by(|a, b| {
        a.port == b.port && (a.pid == b.pid || (a.owner.is_some() && a.owner == b.owner))
    });

    Snapshot { projects, ports }
}

/// The server a listening pid belongs to, its project folder, and the
/// process that names it.
fn server_of(
    pid: u32,
    table: &impl ProcessTable,
    home: &Path,
    protected: &HashSet<u32>,
) -> Option<(ServerId, PathBuf, u32)> {
    if !table.is_mine(pid) || protected.contains(&pid) {
        return None;
    }
    let cwd = table.cwd(pid)?;
    if cwd == home || !cwd.starts_with(home) {
        return None;
    }
    let root_dir = project::root(&cwd, home);
    let in_project = |p: u32| {
        table.is_mine(p)
            && !protected.contains(&p)
            && table.cwd(p).is_some_and(|c| c.starts_with(&root_dir))
    };

    let mut path = vec![pid];
    while let Some(parent) = path.last().and_then(|p| table.parent(*p)) {
        if parent <= 1 || !in_project(parent) {
            break;
        }
        let cmd = clean_cmd(table.cmd(parent));
        if !is_runner(table.name(parent).as_deref(), &cmd) {
            break;
        }
        path.push(parent);
    }
    let top = *path.last()?;
    // A shell wrapper (`zsh -c "npx expo start"`) is the top, but the first
    // real program below it says what the server is.
    let named_by = path
        .iter()
        .rev()
        .copied()
        .find(|p| !is_shell_program(&clean_cmd(table.cmd(*p))))
        .unwrap_or(top);
    let started = table.start_time(top)?;
    Some((ServerId { pid: top, started }, root_dir, named_by))
}

fn is_shell_program(cmd: &[String]) -> bool {
    cmd.first()
        .is_some_and(|c| SHELLS.contains(&base(c).trim_start_matches('-')))
}

fn base(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// True for a process that is part of a dev server's tree: a known runner,
/// `node` running a runner script, or a shell running a script (`sh -c`,
/// `bash ./dev.sh`) rather than waiting for the user.
fn is_runner(name: Option<&str>, cmd: &[String]) -> bool {
    let program = cmd.first().map(|c| base(c)).or(name).unwrap_or_default();
    let program = program.trim_start_matches('-');
    if RUNNERS.contains(&program) {
        return true;
    }
    if INTERPRETERS.contains(&program) {
        let runs_agent = cmd.iter().skip(1).any(|arg| {
            let arg = base(arg);
            AGENTS
                .iter()
                .any(|agent| arg == *agent || arg.starts_with(&format!("{agent}.")))
        });
        return !runs_agent;
    }
    if SHELLS.contains(&program) {
        // An interactive shell has no script and no -c.
        return cmd.iter().skip(1).any(|a| !a.starts_with('-') || a == "-c");
    }
    false
}

fn node_runner(script: &str) -> Option<&'static str> {
    let name = base(script);
    NODE_RUNNER_SCRIPTS
        .iter()
        .find(|(file, _)| name == *file)
        .map(|(_, runner)| *runner)
}

/// Fixes two ways macOS reports command lines oddly: a program that set its
/// own title ("npm run dev" as one argument), and environment variables
/// read past the end of the arguments (`HOME=/Users/...`).
pub fn clean_cmd(cmd: Vec<String>) -> Vec<String> {
    let mut words: Vec<String> = match cmd.split_first() {
        Some((first, rest)) if first.contains(' ') && !first.starts_with('/') => first
            .split_whitespace()
            .map(String::from)
            .chain(rest.iter().cloned())
            .collect(),
        _ => cmd,
    };
    let is_env = |arg: &str| {
        arg.split_once('=').is_some_and(|(key, _)| {
            !key.is_empty()
                && key
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
        })
    };
    if let Some(cut) = words.iter().skip(1).position(|a| is_env(a)) {
        words.truncate(cut + 1);
    }
    words
}

/// "dev" for `npm run dev`, `yarn dev`, `pnpm dev`; "vite" for
/// `node .../node_modules/.bin/vite`; "http.server" for `python3 -m
/// http.server`; otherwise the program's name.
pub fn server_name(cmd: &[String], exe: Option<&str>) -> String {
    let base = |s: &str| -> String {
        Path::new(s)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| s.to_owned())
    };
    let program = cmd
        .first()
        .map(|c| base(c))
        .or_else(|| exe.map(String::from))
        .unwrap_or_else(|| "server".into());
    let mut program = program.trim_start_matches('-').to_owned();
    let mut args: Vec<&str> = cmd.iter().skip(1).map(String::as_str).collect();
    // `node .../yarn.js run dev` is yarn.
    if program == "node"
        && let Some(runner) = args.first().and_then(|s| node_runner(s))
    {
        program = runner.to_owned();
        args.remove(0);
    }
    let first_plain = || args.iter().find(|a| !a.starts_with('-')).copied();

    match program.as_str() {
        "npx" | "bunx" | "pnpx" => first_plain().map_or(program.clone(), String::from),
        "npm" | "pnpm" | "bun" | "yarn" => {
            // `npm run dev` is "dev"; `npm exec expo start` is "expo".
            let after_run = args
                .iter()
                .position(|a| matches!(*a, "run" | "run-script" | "exec" | "x" | "dlx"))
                .and_then(|i| args[i + 1..].iter().find(|a| !a.starts_with('-')).copied());
            after_run
                .or_else(first_plain)
                .map_or(program.clone(), String::from)
        }
        "node" | "deno" | "bun-run" | "ruby" | "php" | "python" | "python3" => {
            if let Some(i) = args.iter().position(|a| *a == "-m")
                && let Some(module) = args.get(i + 1)
            {
                return (*module).to_owned();
            }
            match first_plain() {
                Some(script) => {
                    let name = base(script);
                    name.strip_suffix(".js")
                        .or_else(|| name.strip_suffix(".mjs"))
                        .or_else(|| name.strip_suffix(".ts"))
                        .or_else(|| name.strip_suffix(".py"))
                        .map_or(name.clone(), String::from)
                }
                None => program,
            }
        }
        "cargo" => match args.iter().position(|a| *a == "--bin") {
            Some(i) => args
                .get(i + 1)
                .map_or("cargo run".into(), |b| (*b).to_owned()),
            None => first_plain().map_or("cargo".into(), |sub| format!("cargo {sub}")),
        },
        _ => program,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use pretty_assertions::assert_eq;

    use super::*;

    struct Proc {
        parent: u32,
        cwd: &'static str,
        name: &'static str,
        cmd: &'static str,
        mine: bool,
    }

    #[derive(Default)]
    struct Table(HashMap<u32, Proc>);

    impl Table {
        fn add(mut self, pid: u32, parent: u32, cwd: &'static str, cmd: &'static str) -> Self {
            let name = cmd.split_whitespace().next().unwrap_or_default();
            let name = name.rsplit('/').next().unwrap_or(name);
            let name: &'static str = Box::leak(name.to_owned().into_boxed_str());
            self.0.insert(
                pid,
                Proc {
                    parent,
                    cwd,
                    name,
                    cmd,
                    mine: true,
                },
            );
            self
        }

        fn foreign(mut self, pid: u32, cmd: &'static str) -> Self {
            self.0.insert(
                pid,
                Proc {
                    parent: 1,
                    cwd: "/",
                    name: cmd,
                    cmd,
                    mine: false,
                },
            );
            self
        }
    }

    impl ProcessTable for Table {
        fn parent(&self, pid: u32) -> Option<u32> {
            self.0.get(&pid).map(|p| p.parent)
        }
        fn cwd(&self, pid: u32) -> Option<PathBuf> {
            self.0.get(&pid).map(|p| PathBuf::from(p.cwd))
        }
        fn name(&self, pid: u32) -> Option<String> {
            self.0.get(&pid).map(|p| p.name.to_owned())
        }
        fn cmd(&self, pid: u32) -> Vec<String> {
            self.0
                .get(&pid)
                .map(|p| p.cmd.split_whitespace().map(String::from).collect())
                .unwrap_or_default()
        }
        fn is_mine(&self, pid: u32) -> bool {
            self.0.get(&pid).is_some_and(|p| p.mine)
        }
        fn start_time(&self, pid: u32) -> Option<u64> {
            self.0.contains_key(&pid).then_some(1000 + pid as u64)
        }
        fn tree(&self, root: u32) -> Vec<u32> {
            let mut out = vec![root];
            let mut i = 0;
            while i < out.len() {
                let pid = out[i];
                out.extend(
                    self.0
                        .iter()
                        .filter(|(_, p)| p.parent == pid)
                        .map(|(c, _)| *c),
                );
                i += 1;
            }
            out
        }
        fn usage(&self, pids: &[u32]) -> ResourceUsage {
            ResourceUsage {
                cpu_percent: pids.len() as f32,
                memory_bytes: pids.len() as u64 * 1024,
            }
        }
    }

    fn listener(port: u16, pid: u32, command: &str) -> Listener {
        Listener {
            port,
            pid,
            command: command.into(),
        }
    }

    /// Terminal (outside home) → zsh in the project → npm → sh → node.
    fn terminal_with_npm() -> Table {
        Table::default()
            .add(100, 1, "/", "/Applications/Terminal")
            .add(200, 100, "/home/me/verbatim", "-zsh")
            .add(300, 200, "/home/me/verbatim", "npm run dev")
            .add(400, 300, "/home/me/verbatim", "sh -c ./scripts/dev.sh")
            .add(
                500,
                400,
                "/home/me/verbatim",
                "node /home/me/verbatim/node_modules/.bin/vite",
            )
            .add(600, 500, "/home/me/verbatim", "esbuild --service")
    }

    fn snapshot(listeners: &[Listener], table: &Table) -> Snapshot {
        collect(
            listeners,
            table,
            Path::new("/home/me"),
            &HashSet::new(),
            &HashSet::new(),
        )
    }

    #[test]
    fn a_server_is_the_tree_below_the_users_shell() {
        let table = terminal_with_npm();
        let snap = snapshot(&[listener(5273, 500, "node")], &table);
        assert_eq!(snap.projects.len(), 1);
        let project = &snap.projects[0];
        assert_eq!(
            (project.name.as_str(), project.kind.as_str()),
            ("verbatim", "folder")
        );
        let server = &project.servers[0];
        assert_eq!(
            server.id,
            ServerId {
                pid: 300,
                started: 1300
            }
        );
        assert_eq!(server.name, "dev");
        assert_eq!(server.command, "npm run dev");
        assert_eq!(server.ports, vec![5273]);
        assert_eq!(server.processes, 4, "npm, sh, node and esbuild");
        assert_eq!(snap.ports[0].owner, Some(server.id));
    }

    #[test]
    fn two_listeners_of_one_tree_are_one_server() {
        let table = terminal_with_npm();
        let snap = snapshot(
            &[listener(5273, 500, "node"), listener(24678, 600, "esbuild")],
            &table,
        );
        assert_eq!(snap.servers().count(), 1);
        assert_eq!(
            snap.servers().next().map(|s| s.ports.clone()),
            Some(vec![5273, 24678])
        );
    }

    #[test]
    fn a_port_held_by_two_processes_of_one_server_is_listed_once() {
        let table = terminal_with_npm();
        let snap = snapshot(
            &[listener(5273, 500, "node"), listener(5273, 600, "esbuild")],
            &table,
        );
        assert_eq!(snap.ports.len(), 1);
    }

    #[test]
    fn system_apps_are_foreign_and_only_on_dev_ports() {
        let table = terminal_with_npm()
            .foreign(812, "ControlCenter")
            .foreign(90, "rapportd");
        let snap = snapshot(
            &[
                listener(5000, 812, "ControlCenter"),
                listener(49152, 90, "rapportd"),
            ],
            &table,
        );
        assert!(snap.projects.is_empty());
        assert_eq!(snap.ports.len(), 1);
        assert_eq!((snap.ports[0].port, snap.ports[0].owner), (5000, None));
    }

    #[test]
    fn processes_outside_home_are_not_servers() {
        let table = Table::default().add(10, 1, "/opt/tool", "tool serve");
        let snap = snapshot(&[listener(8000, 10, "tool")], &table);
        assert!(snap.projects.is_empty());
        assert_eq!(snap.ports[0].owner, None);
    }

    #[test]
    fn protected_pids_are_never_part_of_a_server() {
        let table = terminal_with_npm();
        let protected: HashSet<u32> = [300].into();
        let snap = collect(
            &[listener(5273, 500, "node")],
            &table,
            Path::new("/home/me"),
            &HashSet::new(),
            &protected,
        );
        let server = snap.servers().next().expect("server");
        assert_ne!(server.id.pid, 300, "the walk stops below a protected pid");
        assert!(
            server.processes < 4,
            "the protected pid is not counted in the tree"
        );
    }

    #[test]
    fn the_walk_never_climbs_into_an_agent_or_editor() {
        let table = Table::default()
            .add(100, 1, "/", "/Applications/Terminal")
            .add(200, 100, "/home/me/avento", "-zsh")
            .add(300, 200, "/home/me/avento", "claude --resume")
            .add(400, 300, "/home/me/avento", "/bin/zsh -c npx expo start")
            .add(
                500,
                400,
                "/home/me/avento",
                "node /home/me/avento/node_modules/.bin/expo start",
            );
        let snap = snapshot(&[listener(8081, 500, "node")], &table);
        let server = snap.servers().next().expect("server");
        assert_eq!(server.id.pid, 400, "stops below claude");
        assert_eq!(server.processes, 2);
        assert_eq!(
            server.name, "expo",
            "named by the program, not the zsh wrapper"
        );
    }

    #[test]
    fn yarn_running_under_node_is_part_of_the_server() {
        let table = Table::default()
            .add(200, 1, "/home/me/verbatim", "-zsh")
            .add(
                300,
                200,
                "/home/me/verbatim",
                "node /opt/homebrew/libexec/bin/yarn.js run dev",
            )
            .add(400, 300, "/home/me/verbatim", "/bin/sh -c ./scripts/dev.sh")
            .add(
                500,
                400,
                "/home/me/verbatim",
                "node /home/me/verbatim/node_modules/.bin/vite",
            );
        let snap = snapshot(&[listener(5273, 500, "node")], &table);
        let server = snap.servers().next().expect("server");
        assert_eq!(server.id.pid, 300);
        assert_eq!(server.name, "dev");
    }

    #[test]
    fn command_lines_are_cleaned() {
        let cmd = |args: &[&str]| args.iter().map(|a| a.to_string()).collect::<Vec<_>>();
        assert_eq!(
            clean_cmd(cmd(&[
                "npm run dev",
                "COLORTERM=truecolor",
                "HOME=/Users/me"
            ])),
            cmd(&["npm", "run", "dev"])
        );
        assert_eq!(
            clean_cmd(cmd(&["/usr/bin/node", "server.js", "--port=3000"])),
            cmd(&["/usr/bin/node", "server.js", "--port=3000"])
        );
    }

    #[test]
    fn names_from_command_lines() {
        let name = |cmd: &str| {
            let cmd: Vec<String> = cmd.split_whitespace().map(String::from).collect();
            server_name(&cmd, None)
        };
        assert_eq!(name("npm run dev"), "dev");
        assert_eq!(name("/opt/homebrew/bin/pnpm run start:api"), "start:api");
        assert_eq!(name("yarn dev"), "dev");
        assert_eq!(name("npm exec expo start --port 8081"), "expo");
        assert_eq!(name("npx expo start"), "expo");
        assert_eq!(name("node /x/node_modules/.bin/vite --port 3000"), "vite");
        assert_eq!(name("node server.js"), "server");
        assert_eq!(name("python3 -m http.server 8000"), "http.server");
        assert_eq!(name("cargo run --bin api"), "api");
        assert_eq!(name("cargo watch"), "cargo watch");
        assert_eq!(name("/usr/local/bin/rails"), "rails");
        assert_eq!(
            name("node /opt/homebrew/libexec/bin/yarn.js run dev"),
            "dev"
        );
    }
}
