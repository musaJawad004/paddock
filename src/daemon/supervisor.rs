//! Process lifecycle: the state machine in `docs/ARCHITECTURE.md`.
//!
//! - Start: spawn through `pty` with `PORT` set; Starting until the first
//!   output (or 1.5 s alive), then Running.
//! - Stop: SIGTERM to the process group, SIGKILL after 5 s if it is still
//!   there. Kill: SIGKILL at once.
//! - Exit: 0 is Exited, anything else Crashed. Whatever the group left
//!   behind gets SIGKILL right away, so no stray `esbuild` keeps a port.
//! - Port and project changes are saved to config.toml as overrides.
//! - Ports are matched to processes by walking each listener's parents up to
//!   a pid Paddock started.
//!
//! Every child is tracked by a run id; reports from an old run are dropped.
//! Only process groups Paddock started are signalled. A foreign pid is
//! signalled only for `KillPort`, which the TUI confirms with the user, and
//! only after a fresh `lsof` shows it still holds that port.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use nix::sys::signal::{self, Signal};
use nix::unistd::Pid;
use tokio::sync::mpsc;

use super::logs::LineSplitter;
use super::ports::Listener;
use super::pty::{self, Report, Running, Tagged};
use super::stats::Stats;
use crate::config::{self, Config, ProcessOverride, process_key};
use crate::detect;
use crate::ipc::protocol::{Event, Request};
use crate::model::{
    ListeningPort, ProcessId, ProcessInfo, ProcessSpec, ProcessState, ProjectInfo, ResourceUsage,
    Snapshot, now_ms,
};

const STOP_GRACE: Duration = Duration::from_secs(5);
const FOREIGN_GRACE: Duration = Duration::from_secs(2);
const QUIET_START: Duration = Duration::from_millis(1500);
/// Lines kept per process for a TUI that connects later (v0.2 daemon).
const HISTORY: usize = 2_000;

struct Project {
    path: PathBuf,
    name: String,
}

struct StopPlan {
    kill_at: Instant,
    then_start: bool,
}

struct Proc {
    project: PathBuf,
    spec: ProcessSpec,
    over: ProcessOverride,
    state: ProcessState,
    run: Option<(u64, Running)>,
    spawned_at: Option<Instant>,
    started_at_ms: Option<u64>,
    restarts: u32,
    usage: Option<ResourceUsage>,
    /// The port its tree really listens on, from the last scan.
    observed_port: Option<u16>,
    splitter: LineSplitter,
    history: VecDeque<String>,
    stop: Option<StopPlan>,
    killed: bool,
}

impl Proc {
    fn port(&self) -> Option<u16> {
        self.over.port.or(self.spec.port)
    }

    fn command(&self) -> String {
        match (self.over.port, &self.spec.port_args) {
            (Some(port), Some(args)) => {
                format!(
                    "{} {}",
                    self.spec.command,
                    args.replace("{port}", &port.to_string())
                )
            }
            _ => self.spec.command.clone(),
        }
    }

    fn pid(&self) -> Option<u32> {
        self.run.as_ref().map(|(_, running)| running.pid)
    }
}

pub struct Supervisor {
    config_path: PathBuf,
    projects: Vec<Project>,
    procs: Vec<Proc>,
    listeners: Vec<ListeningPort>,
    foreign_kills: Vec<(u32, Instant)>,
    next_run: u64,
    reports: mpsc::Sender<Tagged>,
    /// Set when something changed that the next port scan should show soon.
    pub wants_scan: bool,
}

impl Supervisor {
    pub fn new(config_path: PathBuf, reports: mpsc::Sender<Tagged>) -> Self {
        Self {
            config_path,
            projects: Vec::new(),
            procs: Vec::new(),
            listeners: Vec::new(),
            foreign_kills: Vec::new(),
            next_run: 1,
            reports,
            wants_scan: true,
        }
    }

    /// Adds every configured project. Reads files: call from a blocking
    /// context.
    pub fn load(&mut self, config: &Config) {
        for path in &config.projects {
            self.add_project_dir(&config::expand_home(path), config);
        }
    }

    fn add_project_dir(&mut self, path: &Path, config: &Config) -> usize {
        let name = detect::project_name(path);
        let specs = detect::detect(path);
        let found = specs.len();
        for spec in specs {
            let over = config
                .processes
                .get(&process_key(path, &spec.name))
                .cloned()
                .unwrap_or_default();
            self.procs.push(Proc {
                project: path.to_owned(),
                spec,
                over,
                state: ProcessState::Stopped,
                run: None,
                spawned_at: None,
                started_at_ms: None,
                restarts: 0,
                usage: None,
                observed_port: None,
                splitter: LineSplitter::default(),
                history: VecDeque::new(),
                stop: None,
                killed: false,
            });
        }
        self.projects.push(Project {
            path: path.to_owned(),
            name,
        });
        found
    }

    pub fn is_empty(&self) -> bool {
        self.projects.is_empty()
    }

    fn project_name(&self, path: &Path) -> String {
        self.projects
            .iter()
            .find(|p| p.path == path)
            .map_or_else(|| detect::project_name(path), |p| p.name.clone())
    }

    fn id(&self, proc: &Proc) -> ProcessId {
        let group = proc
            .over
            .group
            .clone()
            .unwrap_or_else(|| self.project_name(&proc.project));
        ProcessId::new(group, proc.spec.name.clone())
    }

    fn find(&self, id: &ProcessId) -> Option<usize> {
        self.procs.iter().position(|p| &self.id(p) == id)
    }

    fn info(&self, proc: &Proc) -> ProcessInfo {
        ProcessInfo {
            id: self.id(proc),
            command: proc.command(),
            state: proc.state,
            port: proc.observed_port.or(proc.port()),
            usage: proc.usage,
            cwd: proc.spec.cwd.clone(),
            source: proc.spec.source.clone(),
            pid: proc.pid(),
            started_at_ms: proc.started_at_ms,
            restarts: proc.restarts,
        }
    }

    pub fn snapshot(&self) -> Snapshot {
        self.snapshot_with(&self.listeners)
    }

    fn snapshot_with(&self, ports: &[ListeningPort]) -> Snapshot {
        let mut groups: Vec<(String, PathBuf)> = self
            .projects
            .iter()
            .map(|p| (p.name.clone(), p.path.clone()))
            .collect();
        for proc in &self.procs {
            if let Some(group) = &proc.over.group
                && !groups.iter().any(|(name, _)| name == group)
            {
                groups.push((group.clone(), PathBuf::new()));
            }
        }
        let projects = groups
            .into_iter()
            .map(|(name, path)| ProjectInfo {
                processes: self
                    .procs
                    .iter()
                    .filter(|p| self.id(p).project == name)
                    .map(|p| self.info(p))
                    .collect(),
                name,
                path,
            })
            .collect();
        Snapshot {
            projects,
            ports: ports.to_vec(),
        }
    }

    /// Output kept so far, for a TUI that just connected.
    pub fn history(&self) -> Vec<Event> {
        self.procs
            .iter()
            .filter(|p| !p.history.is_empty())
            .map(|p| Event::Output {
                id: self.id(p),
                lines: p.history.iter().cloned().collect(),
            })
            .collect()
    }

    pub fn handle(&mut self, request: Request, now: Instant) -> Vec<Event> {
        let target = |id: &ProcessId| self.find(id);
        match request {
            Request::Start(id) => match target(&id) {
                Some(i) => self.start(i, now),
                None => unknown(&id),
            },
            Request::Stop(id) => match target(&id) {
                Some(i) => self.stop(i, false, now),
                None => unknown(&id),
            },
            Request::Restart(id) => match target(&id) {
                Some(i) if self.procs[i].run.is_some() => self.stop(i, true, now),
                Some(i) => self.start(i, now),
                None => unknown(&id),
            },
            Request::Kill(id) => match target(&id) {
                Some(i) => self.kill(i),
                None => unknown(&id),
            },
            Request::SetPort { id, port } => match target(&id) {
                Some(i) => self.set_port(i, port, now),
                None => unknown(&id),
            },
            Request::Move { id, project } => match target(&id) {
                Some(i) => self.move_to(i, project),
                None => unknown(&id),
            },
            Request::KillPort { port, pid } => self.kill_port(port, pid, now),
            Request::AddProject(path) => self.add_project(&path),
            Request::RemoveProject(name) => self.remove_project(&name),
        }
    }

    fn start(&mut self, i: usize, now: Instant) -> Vec<Event> {
        let id = self.id(&self.procs[i]);
        let proc = &mut self.procs[i];
        if proc.run.is_some() {
            return vec![Event::Notice(format!("{id} is already running."))];
        }
        let command = proc.command();
        let mut env = proc.spec.env.clone();
        if let Some(port) = proc.port() {
            env.push(("PORT".into(), port.to_string()));
        }
        let run = self.next_run;
        self.next_run += 1;

        let header = format!("\u{1b}[2m$ {command}\u{1b}[22m");
        push_history(&mut proc.history, header.clone());
        let mut events = vec![Event::Output {
            id: id.clone(),
            lines: vec![header],
        }];
        match pty::spawn(&command, &proc.spec.cwd, &env, run, self.reports.clone()) {
            Ok(running) => {
                proc.run = Some((run, running));
                proc.spawned_at = Some(now);
                proc.started_at_ms = Some(now_ms());
                proc.killed = false;
                proc.stop = None;
                proc.splitter = LineSplitter::default();
                events.extend(self.set_state(i, ProcessState::Starting));
                events.push(Event::Snapshot(self.snapshot()));
            }
            Err(err) => {
                let line = format!("\u{1b}[31mCould not start: {err}\u{1b}[39m");
                push_history(&mut proc.history, line.clone());
                events.push(Event::Output {
                    id: id.clone(),
                    lines: vec![line],
                });
                events.extend(self.set_state(i, ProcessState::Crashed(Some(127))));
            }
        }
        events
    }

    fn stop(&mut self, i: usize, then_start: bool, now: Instant) -> Vec<Event> {
        let id = self.id(&self.procs[i]);
        let proc = &mut self.procs[i];
        let Some(pid) = proc.pid() else {
            return vec![Event::Notice(format!("{id} is not running."))];
        };
        if let Some(plan) = &mut proc.stop {
            plan.then_start |= then_start;
            return Vec::new();
        }
        let _ = pty::signal_group(pid, Signal::SIGTERM);
        proc.stop = Some(StopPlan {
            kill_at: now + STOP_GRACE,
            then_start,
        });
        self.set_state(i, ProcessState::Stopping)
    }

    fn kill(&mut self, i: usize) -> Vec<Event> {
        let id = self.id(&self.procs[i]);
        let proc = &mut self.procs[i];
        let Some(pid) = proc.pid() else {
            return vec![Event::Notice(format!("{id} is not running."))];
        };
        proc.killed = true;
        proc.stop = None;
        let _ = pty::signal_group(pid, Signal::SIGKILL);
        Vec::new()
    }

    fn set_port(&mut self, i: usize, port: u16, now: Instant) -> Vec<Event> {
        let id = self.id(&self.procs[i]);
        if let Some(taken) = self
            .listeners
            .iter()
            .find(|l| l.port == port && l.owner.as_ref() != Some(&id))
        {
            let who = taken.owner.as_ref().map_or(
                format!("{} (pid {})", taken.command, taken.pid),
                ProcessId::to_string,
            );
            return vec![Event::Notice(format!(
                "Port {port} is already used by {who}."
            ))];
        }
        let proc = &mut self.procs[i];
        proc.over.port = (Some(port) != proc.spec.port).then_some(port);
        let mut events = self.save_override(i);
        events.push(Event::Notice(format!("{id} will use port {port}.")));
        if self.procs[i].run.is_some() {
            events.extend(self.stop(i, true, now));
        }
        events.push(Event::Snapshot(self.snapshot()));
        events
    }

    fn move_to(&mut self, i: usize, group: String) -> Vec<Event> {
        let from = self.id(&self.procs[i]);
        let to = ProcessId::new(group.clone(), from.name.clone());
        if self.find(&to).is_some() {
            return vec![Event::Notice(format!(
                "{group} already has a process called {}.",
                from.name
            ))];
        }
        let own = self.project_name(&self.procs[i].project);
        self.procs[i].over.group = (group != own).then_some(group.clone());
        let mut events = self.save_override(i);
        events.push(Event::Renamed {
            from: from.clone(),
            to,
        });
        events.push(Event::Snapshot(self.snapshot()));
        events.push(Event::Notice(format!("Moved {} to {group}.", from.name)));
        events
    }

    fn save_override(&mut self, i: usize) -> Vec<Event> {
        let proc = &self.procs[i];
        let key = process_key(&proc.project, &proc.spec.name);
        let over = proc.over.clone();
        let saved = config::update_at(&self.config_path, |config| {
            if over.is_empty() {
                config.processes.remove(&key);
            } else {
                config.processes.insert(key, over);
            }
        });
        match saved {
            Ok(_) => Vec::new(),
            Err(err) => vec![Event::Notice(format!("Could not save config.toml: {err}"))],
        }
    }

    fn kill_port(&mut self, port: u16, pid: u32, now: Instant) -> Vec<Event> {
        let fresh = match super::ports::scan() {
            Ok(listeners) => listeners,
            Err(err) => return vec![Event::Notice(err)],
        };
        let Some(listener) = fresh.iter().find(|l| l.port == port && l.pid == pid) else {
            return vec![Event::Notice(format!(
                "pid {pid} no longer listens on port {port}."
            ))];
        };
        if let Some(i) = self.procs.iter().position(|p| self.owns(p, pid)) {
            return self.kill(i);
        }
        if pid <= 1 || pid == std::process::id() {
            return vec![Event::Notice(format!("Refusing to kill pid {pid}."))];
        }
        let Ok(raw) = i32::try_from(pid) else {
            return Vec::new();
        };
        match signal::kill(Pid::from_raw(raw), Signal::SIGTERM) {
            Ok(()) => {
                self.foreign_kills.push((pid, now + FOREIGN_GRACE));
                self.wants_scan = true;
                vec![Event::Notice(format!(
                    "Asked {} (pid {pid}) to quit. Port {port} should be free in a moment.",
                    listener.command
                ))]
            }
            Err(nix::errno::Errno::EPERM) => vec![Event::Notice(format!(
                "Not allowed to kill pid {pid}: it belongs to another user."
            ))],
            Err(err) => vec![Event::Notice(format!("Could not kill pid {pid}: {err}"))],
        }
    }

    /// True if `pid` is the root of `proc`'s current run. Listener owners
    /// from the last scan cover the rest of the tree.
    fn owns(&self, proc: &Proc, pid: u32) -> bool {
        proc.pid() == Some(pid)
            || self
                .listeners
                .iter()
                .any(|l| l.pid == pid && l.owner.as_ref() == Some(&self.id(proc)))
    }

    fn add_project(&mut self, path: &Path) -> Vec<Event> {
        let expanded = config::expand_home(path);
        let Ok(path) = expanded.canonicalize() else {
            return vec![Event::Notice(format!(
                "{} does not exist.",
                expanded.display()
            ))];
        };
        if !path.is_dir() {
            return vec![Event::Notice(format!(
                "{} is not a folder.",
                path.display()
            ))];
        }
        if self.projects.iter().any(|p| p.path == path) {
            return vec![Event::Notice(format!(
                "{} is already in Paddock.",
                detect::project_name(&path)
            ))];
        }
        let saved = config::update_at(&self.config_path, |config| {
            config.projects.push(path.clone());
        });
        let config = match saved {
            Ok(config) => config,
            Err(err) => return vec![Event::Notice(format!("Could not save config.toml: {err}"))],
        };
        let found = self.add_project_dir(&path, &config);
        let name = detect::project_name(&path);
        let notice = match found {
            0 => format!(
                "Added {name}, but found nothing to run. Add a paddock.toml to say what to run."
            ),
            1 => format!("Added {name}: 1 process. Press s to start it."),
            n => format!("Added {name}: {n} processes."),
        };
        vec![Event::Snapshot(self.snapshot()), Event::Notice(notice)]
    }

    fn remove_project(&mut self, name: &str) -> Vec<Event> {
        let Some(index) = self.projects.iter().position(|p| p.name == name) else {
            return vec![Event::Notice(format!("{name} is not a project folder."))];
        };
        let path = self.projects[index].path.clone();
        if self
            .procs
            .iter()
            .any(|p| p.project == path && p.run.is_some())
        {
            return vec![Event::Notice(format!(
                "Stop the processes in {name} first."
            ))];
        }
        let saved = config::update_at(&self.config_path, |config| {
            config.projects.retain(|p| config::expand_home(p) != path);
        });
        if let Err(err) = saved {
            return vec![Event::Notice(format!("Could not save config.toml: {err}"))];
        }
        self.projects.remove(index);
        self.procs.retain(|p| p.project != path);
        vec![
            Event::Snapshot(self.snapshot()),
            Event::Notice(format!(
                "Removed {name} from Paddock. Its files are untouched."
            )),
        ]
    }

    pub fn on_report(&mut self, tagged: Tagged, now: Instant) -> Vec<Event> {
        let Some(i) = self
            .procs
            .iter()
            .position(|p| p.run.as_ref().is_some_and(|(run, _)| *run == tagged.run))
        else {
            return Vec::new();
        };
        match tagged.report {
            Report::Bytes(bytes) => {
                let lines = self.procs[i].splitter.push(&bytes, now);
                let mut events = self.output(i, lines);
                if self.procs[i].state == ProcessState::Starting {
                    events.extend(self.set_state(i, ProcessState::Running));
                }
                events
            }
            Report::Exited(code) => self.on_exit(i, code, now),
        }
    }

    fn on_exit(&mut self, i: usize, code: Option<i32>, now: Instant) -> Vec<Event> {
        let proc = &mut self.procs[i];
        let Some((_, running)) = proc.run.take() else {
            return Vec::new();
        };
        // Whatever the group left behind (a server the script started) goes
        // too, so it cannot keep the port.
        let _ = pty::signal_group(running.pid, Signal::SIGKILL);
        drop(running);

        let rest = proc.splitter.finish();
        let plan = proc.stop.take();
        let (state, note) = if proc.killed {
            (ProcessState::Crashed(None), "killed".to_owned())
        } else if plan.is_some() {
            (ProcessState::Stopped, "stopped".to_owned())
        } else {
            match code {
                Some(0) => (ProcessState::Exited, "exited".to_owned()),
                Some(code) => (
                    ProcessState::Crashed(Some(code)),
                    format!("exited with code {code}"),
                ),
                None => (ProcessState::Crashed(None), "ended by a signal".to_owned()),
            }
        };
        proc.spawned_at = None;
        proc.started_at_ms = None;
        proc.usage = None;
        proc.observed_port = None;

        let mut lines: Vec<String> = rest.into_iter().collect();
        lines.push(format!("\u{1b}[2m[{note}]\u{1b}[22m"));
        let mut events = self.output(i, lines);
        events.extend(self.set_state(i, state));
        events.push(Event::Snapshot(self.snapshot()));
        self.wants_scan = true;
        if plan.is_some_and(|p| p.then_start) {
            self.procs[i].restarts += 1;
            events.extend(self.start(i, now));
        }
        events
    }

    pub fn on_tick(&mut self, now: Instant) -> Vec<Event> {
        let mut events = Vec::new();
        for i in 0..self.procs.len() {
            if let Some(line) = self.procs[i].splitter.flush_stale(now) {
                events.extend(self.output(i, vec![line]));
            }
            let proc = &self.procs[i];
            if proc.state == ProcessState::Starting
                && proc
                    .spawned_at
                    .is_some_and(|at| now.duration_since(at) >= QUIET_START)
            {
                events.extend(self.set_state(i, ProcessState::Running));
            }
            let proc = &mut self.procs[i];
            if let (Some(plan), Some(pid)) = (&proc.stop, proc.pid())
                && now >= plan.kill_at
            {
                let _ = pty::signal_group(pid, Signal::SIGKILL);
                proc.stop = Some(StopPlan {
                    kill_at: now + STOP_GRACE,
                    then_start: plan.then_start,
                });
            }
        }
        self.foreign_kills.retain(|(pid, at)| {
            if now < *at {
                return true;
            }
            if let Ok(raw) = i32::try_from(*pid) {
                // Signal 0 only checks that it still exists.
                if signal::kill(Pid::from_raw(raw), None).is_ok() {
                    let _ = signal::kill(Pid::from_raw(raw), Signal::SIGKILL);
                }
            }
            false
        });
        events
    }

    /// Matches listeners to processes and sums CPU and memory per tree.
    pub fn apply_scan(&mut self, listeners: &[Listener], stats: &Stats) -> Vec<Event> {
        let roots: HashMap<u32, usize> = self
            .procs
            .iter()
            .enumerate()
            .filter_map(|(i, p)| Some((p.pid()?, i)))
            .collect();
        let owner_of = |pid: u32| -> Option<usize> {
            let mut current = pid;
            for _ in 0..64 {
                if let Some(i) = roots.get(&current) {
                    return Some(*i);
                }
                current = stats.parent(current)?;
            }
            None
        };

        let mut owners: HashMap<u32, usize> = HashMap::new();
        let mut ports: Vec<ListeningPort> = listeners
            .iter()
            .map(|l| {
                let owner = owner_of(l.pid).or_else(|| self.docker_owner(l));
                if let Some(i) = owner {
                    owners.insert(l.pid, i);
                }
                ListeningPort {
                    port: l.port,
                    pid: l.pid,
                    command: l.command.clone(),
                    owner: owner.map(|i| self.id(&self.procs[i])),
                }
            })
            .collect();
        // Only ports that matter here: ours, and foreign ones on a port one
        // of our processes wants.
        let wanted: Vec<u16> = self.procs.iter().filter_map(Proc::port).collect();
        ports.retain(|p| p.owner.is_some() || wanted.contains(&p.port));
        ports.sort_by_key(|p| p.port);
        ports.dedup_by_key(|p| (p.port, p.pid));

        let mut changed_ports = false;
        for (i, proc) in self.procs.iter_mut().enumerate() {
            let observed = ports
                .iter()
                .filter(|p| p.owner.is_some() && owners.get(&p.pid) == Some(&i))
                .map(|p| p.port)
                .min_by_key(|port| Some(*port) != proc.over.port.or(proc.spec.port));
            if observed != proc.observed_port {
                proc.observed_port = observed;
                changed_ports = true;
            }
        }

        let mut usage = Vec::new();
        for proc in &mut self.procs {
            if let Some(pid) = proc.pid() {
                proc.usage = Some(stats.tree_usage(pid));
            }
        }
        for proc in &self.procs {
            if let Some(u) = proc.usage.filter(|_| proc.run.is_some()) {
                usage.push((self.id(proc), u));
            }
        }

        let mut events = Vec::new();
        if changed_ports {
            events.push(Event::Snapshot(self.snapshot_with(&ports)));
        }
        if ports != self.listeners {
            self.listeners = ports.clone();
            events.push(Event::Ports(ports));
        }
        events.push(Event::Usage(usage));
        events
    }

    /// Docker publishes container ports from its own daemon, not from the
    /// `docker compose up` Paddock started. Credit the port to the compose
    /// process that expects it.
    fn docker_owner(&self, listener: &Listener) -> Option<usize> {
        let command = listener.command.to_ascii_lowercase();
        if !(command.contains("docker")
            || command.contains("vpnkit")
            || command.contains("orbstack"))
        {
            return None;
        }
        self.procs.iter().position(|p| {
            p.run.is_some() && p.port() == Some(listener.port) && p.spec.command.contains("docker")
        })
    }

    fn output(&mut self, i: usize, lines: Vec<String>) -> Vec<Event> {
        if lines.is_empty() {
            return Vec::new();
        }
        for line in &lines {
            push_history(&mut self.procs[i].history, line.clone());
        }
        vec![Event::Output {
            id: self.id(&self.procs[i]),
            lines,
        }]
    }

    fn set_state(&mut self, i: usize, state: ProcessState) -> Vec<Event> {
        self.procs[i].state = state;
        vec![Event::State {
            id: self.id(&self.procs[i]),
            state,
        }]
    }

    /// Starts stopping everything. Returns how many are still running.
    pub fn stop_all(&mut self, now: Instant) -> usize {
        for i in 0..self.procs.len() {
            if self.procs[i].run.is_some() && self.procs[i].stop.is_none() {
                let _ = self.stop(i, false, now);
            }
        }
        self.running()
    }

    pub fn running(&self) -> usize {
        self.procs.iter().filter(|p| p.run.is_some()).count()
    }

    pub fn kill_all(&mut self) {
        for proc in &mut self.procs {
            if let Some(pid) = proc.pid() {
                let _ = pty::signal_group(pid, Signal::SIGKILL);
            }
        }
    }
}

impl Drop for Supervisor {
    /// Last line of defence: if the supervisor goes away without a clean
    /// shutdown, nothing it started keeps running.
    fn drop(&mut self) {
        self.kill_all();
    }
}

fn unknown(id: &ProcessId) -> Vec<Event> {
    vec![Event::Notice(format!("{id} is not a known process."))]
}

fn push_history(history: &mut VecDeque<String>, line: String) {
    history.push_back(line);
    if history.len() > HISTORY {
        history.pop_front();
    }
}
