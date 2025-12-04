use std::collections::{HashMap, hash_map};
use std::io::BufRead;

use anyhow::Context;
use rustc_hash::FxBuildHasher;

use crate::tosubstr::ToSubStr;
use crate::walk::Node;

pub type Pid = i32;
pub type ProcessTree = HashMap<Pid, Process, FxBuildHasher>;

const PROCESS_TREE_CAPACITY: usize = 2048;

#[derive(Default, Clone, Debug)]
pub struct ProcessInfo {
    pub pid: Pid,
    pub name: String,
}

impl ProcessInfo {
    pub fn new(pid: Pid, name: String) -> Self {
        Self { pid, name }
    }
}

#[derive(Clone, Debug)]
pub struct Process {
    info: ProcessInfo,
    children: Vec<Pid>,
}

impl Process {
    pub fn new(info: ProcessInfo) -> Self {
        Self {
            info,
            children: vec![],
        }
    }

    pub fn new_with_children(info: ProcessInfo, children: Vec<Pid>) -> Self {
        Self { info, children }
    }
}

impl Node<ProcessInfo> for Process {
    type Context = ProcessTree;

    fn data(&self) -> &ProcessInfo {
        &self.info
    }

    fn data_mut(&mut self) -> &mut ProcessInfo {
        &mut self.info
    }

    fn children<'a>(&'a self, tree: &'a Self::Context) -> impl Iterator<Item = &'a Self> {
        self.children.iter().filter_map(|pid| tree.get(pid))
    }
}

#[derive(Debug, Clone)]
struct Status {
    /// Command run by this process.
    pub name: String,
}

impl procfs::FromBufRead for Status {
    fn from_buf_read<R: BufRead>(mut reader: R) -> procfs::ProcResult<Self> {
        let mut line = "".to_string();
        while reader.read_line(&mut line)? != 0 {
            let Some(name) = line.strip_prefix("Name:") else {
                line.clear();
                continue;
            };

            let range = line.substr_range(name.trim()).expect("name is within line");
            line.to_substr(range);
            let status = Status { name: line };
            return Ok(status);
        }

        let err = procfs::ProcError::NotFound(None);
        Err(err)
    }
}

pub fn build_process_tree() -> anyhow::Result<ProcessTree> {
    let mut processes = ProcessTree::with_capacity_and_hasher(PROCESS_TREE_CAPACITY, FxBuildHasher);
    for proc in procfs::process::all_processes().context("read /proc")? {
        // Process could die by the time we come to it, it's normal
        let Ok(proc) = proc else { continue };

        let stat = proc.stat().context("read stat file")?;
        let status = proc
            .read::<_, Status>("status")
            .context("read status file")?;
        let info = ProcessInfo::new(proc.pid(), status.name);

        match processes.entry(proc.pid()) {
            hash_map::Entry::Occupied(mut e) => {
                e.get_mut().info = info;
            }
            hash_map::Entry::Vacant(e) => {
                e.insert(Process::new(info));
            }
        }

        // skipping root process because it's `children` vec is going to be huge and useless
        if stat.ppid == 1 {
            continue;
        }

        processes
            .entry(stat.ppid)
            .and_modify(|pproc| pproc.children.push(proc.pid()))
            .or_insert(Process::new_with_children(
                Default::default(),
                vec![proc.pid],
            ));
    }

    Ok(processes)
}
