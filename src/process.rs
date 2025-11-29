use std::collections::HashMap;

use sysinfo::Pid;

use crate::walk::Node;

pub type ProcessTree<'a> = HashMap<Pid, Process<'a>>;

#[derive(Clone, Debug)]
pub struct Process<'a> {
    process: &'a sysinfo::Process,
    children: Vec<Pid>,
}

impl<'a> Process<'a> {
    pub fn new(process: &'a sysinfo::Process) -> Self {
        Self {
            process,
            children: vec![],
        }
    }

    pub fn new_with_children(process: &'a sysinfo::Process, children: Vec<Pid>) -> Self {
        Self { process, children }
    }
}

impl<'a> Node<&'a sysinfo::Process> for Process<'a> {
    type Context = ProcessTree<'a>;

    fn data(&self) -> &&'a sysinfo::Process {
        &self.process
    }

    fn data_mut(&mut self) -> &mut &'a sysinfo::Process {
        &mut self.process
    }

    fn children<'b>(&'b self, tree: &'b Self::Context) -> impl Iterator<Item = &'b Self> {
        self.children.iter().filter_map(|pid| tree.get(pid))
    }
}

pub fn build_process_tree(sys: &sysinfo::System) -> ProcessTree<'_> {
    let mut processes = ProcessTree::new();
    for (&pid, process) in sys.processes() {
        processes.entry(pid).or_insert(Process::new(process));

        if let Some(parent) = process.parent().and_then(|ppid| sys.processes().get(&ppid)) {
            processes
                .entry(parent.pid())
                .and_modify(|proc| proc.children.push(pid))
                .or_insert(Process::new_with_children(parent, vec![pid]));
        }
    }

    processes
}
