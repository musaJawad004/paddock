//! The process table, from `sysinfo`: parents, working folders, command
//! lines, owners and start times for grouping servers, and CPU and memory
//! summed over a whole process tree.
//!
//! CPU percentages need two samples, so one `Stats` lives for the whole run
//! and is refreshed on the scan tick. Blocking: call from a blocking task.

use std::collections::HashMap;
use std::path::PathBuf;

use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

use super::servers::ProcessTable;
use crate::model::ResourceUsage;

pub struct Stats {
    system: System,
    /// Children of each pid, rebuilt on every refresh.
    children: HashMap<u32, Vec<u32>>,
    me: u32,
}

impl Default for Stats {
    fn default() -> Self {
        Self::new()
    }
}

impl Stats {
    pub fn new() -> Self {
        Self {
            system: System::new(),
            children: HashMap::new(),
            me: nix::unistd::getuid().as_raw(),
        }
    }

    pub fn refresh(&mut self) {
        self.system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing()
                .with_cpu()
                .with_memory()
                .with_cwd(UpdateKind::OnlyIfNotSet)
                .with_cmd(UpdateKind::OnlyIfNotSet)
                .with_user(UpdateKind::OnlyIfNotSet),
        );
        self.children.clear();
        for (pid, process) in self.system.processes() {
            if let Some(parent) = process.parent() {
                self.children
                    .entry(parent.as_u32())
                    .or_default()
                    .push(pid.as_u32());
            }
        }
    }

    fn process(&self, pid: u32) -> Option<&sysinfo::Process> {
        self.system.process(Pid::from_u32(pid))
    }

    /// Paddock itself and every process above it. Never part of a server,
    /// never signalled.
    pub fn own_line(&self) -> Vec<u32> {
        let mut line = vec![std::process::id()];
        while let Some(parent) = line.last().and_then(|p| self.parent(*p)) {
            if parent <= 1 || line.contains(&parent) {
                break;
            }
            line.push(parent);
        }
        line
    }
}

impl ProcessTable for Stats {
    fn parent(&self, pid: u32) -> Option<u32> {
        self.process(pid)?.parent().map(|p| p.as_u32())
    }

    fn cwd(&self, pid: u32) -> Option<PathBuf> {
        self.process(pid)?.cwd().map(PathBuf::from)
    }

    fn name(&self, pid: u32) -> Option<String> {
        Some(self.process(pid)?.name().to_string_lossy().into_owned())
    }

    fn cmd(&self, pid: u32) -> Vec<String> {
        self.process(pid)
            .map(|p| {
                p.cmd()
                    .iter()
                    .map(|a| a.to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn is_mine(&self, pid: u32) -> bool {
        self.process(pid)
            .and_then(|p| p.user_id())
            .is_some_and(|uid| **uid == self.me)
    }

    fn start_time(&self, pid: u32) -> Option<u64> {
        Some(self.process(pid)?.start_time())
    }

    fn tree(&self, root: u32) -> Vec<u32> {
        let mut out = vec![root];
        let mut i = 0;
        while let Some(pid) = out.get(i).copied() {
            for child in self.children.get(&pid).into_iter().flatten() {
                if !out.contains(child) {
                    out.push(*child);
                }
            }
            i += 1;
        }
        out
    }

    fn usage(&self, pids: &[u32]) -> ResourceUsage {
        let mut usage = ResourceUsage {
            cpu_percent: 0.0,
            memory_bytes: 0,
        };
        for process in pids.iter().filter_map(|p| self.process(*p)) {
            usage.cpu_percent += process.cpu_usage();
            usage.memory_bytes += process.memory();
        }
        usage
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sees_its_own_process() {
        let mut stats = Stats::new();
        stats.refresh();
        let me = std::process::id();
        assert!(stats.is_mine(me));
        assert!(stats.tree(me).contains(&me));
        assert!(stats.usage(&[me]).memory_bytes > 0);
        assert_eq!(stats.cwd(me), std::env::current_dir().ok());
        assert!(stats.start_time(me).is_some());
        assert_eq!(stats.own_line()[0], me);
    }
}
