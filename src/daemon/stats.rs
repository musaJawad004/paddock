//! The process table, from `sysinfo`: parents for walking a listener back to
//! the process Paddock started, and CPU and memory summed over a whole
//! process tree (`yarn` plus the `node` and `esbuild` under it).
//!
//! CPU percentages need two samples, so one `Stats` lives for the whole run
//! and is refreshed on the scan tick. Blocking: call from a blocking task.

use std::collections::HashMap;

use std::path::Path;

use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

use crate::model::ResourceUsage;

pub struct Stats {
    system: System,
    /// Children of each pid, rebuilt on every refresh.
    children: HashMap<u32, Vec<u32>>,
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

    pub fn parent(&self, pid: u32) -> Option<u32> {
        self.system
            .process(sysinfo::Pid::from_u32(pid))?
            .parent()
            .map(|p| p.as_u32())
    }

    /// `pid` and every process below it.
    pub fn tree(&self, root: u32) -> Vec<u32> {
        let mut out = vec![root];
        let mut i = 0;
        while let Some(pid) = out.get(i).copied() {
            if let Some(children) = self.children.get(&pid) {
                out.extend(
                    children
                        .iter()
                        .filter(|c| !out.contains(c))
                        .copied()
                        .collect::<Vec<_>>(),
                );
            }
            i += 1;
        }
        out
    }

    pub fn tree_usage(&self, root: u32) -> ResourceUsage {
        let mut usage = ResourceUsage {
            cpu_percent: 0.0,
            memory_bytes: 0,
        };
        for pid in self.tree(root) {
            if let Some(process) = self.system.process(sysinfo::Pid::from_u32(pid)) {
                usage.cpu_percent += process.cpu_usage();
                usage.memory_bytes += process.memory();
            }
        }
        usage
    }

    /// The folder a process runs in, when the OS lets us see it.
    pub fn cwd(&self, pid: u32) -> Option<&Path> {
        self.system.process(sysinfo::Pid::from_u32(pid))?.cwd()
    }

    /// True if the process belongs to the user running Paddock.
    pub fn is_mine(&self, pid: u32) -> bool {
        let me = nix::unistd::getuid().as_raw();
        self.system
            .process(sysinfo::Pid::from_u32(pid))
            .and_then(|p| p.user_id())
            .is_some_and(|uid| **uid == me)
    }

    pub fn is_alive(&self, pid: u32) -> bool {
        self.system.process(sysinfo::Pid::from_u32(pid)).is_some()
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
        assert!(stats.is_alive(me));
        assert!(stats.tree(me).contains(&me));
        assert!(stats.tree_usage(me).memory_bytes > 0);
        assert!(stats.is_mine(me));
        assert_eq!(stats.cwd(me), std::env::current_dir().ok().as_deref());
    }
}
