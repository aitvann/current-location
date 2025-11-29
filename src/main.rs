use std::fs::File;
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::{env, fs, io};

use anyhow::Context;
use clap::{Parser, Subcommand};
use hyprland::data::Client;
use hyprland::shared::HyprDataActiveOptional;
use serde::{Deserialize, Serialize};
use serde_with::{FromInto, serde_as};
use sysinfo::{Pid, ProcessRefreshKind, RefreshKind, System};

use crate::process::Process;
use crate::walk::{ContinueFlow, Node, Walker, WalkerNode};

mod process;
mod walk;

const KNOWN_PROCS: &[&str] = &["zsh", "nvim"];
static LOCATIONS_PATH: LazyLock<&Path> = LazyLock::new(|| Path::new("/tmp/current-location/"));

#[derive(Parser)]
struct Opts {
    #[clap(subcommand)]
    subcommand: Subcommands,
}

#[derive(Subcommand, Clone)]
enum Subcommands {
    Get,
    Write {
        name: String,
        pid: Pid,
        location: PathBuf,
        nvim_pipe: Option<Pid>,
    },
    Clear,
}

#[serde_as]
#[derive(Serialize, Deserialize, Clone, Debug)]
struct LocationData {
    location: PathBuf,
    #[serde_as(as = "Option<FromInto<usize>>")]
    nvim_pipe: Option<Pid>,
}

#[derive(Clone, Debug)]
struct LocationSearch<'a> {
    known_procs: Vec<&'a sysinfo::Process>,
}

impl<'a> LocationSearch<'a> {
    fn new() -> Self {
        Self {
            known_procs: Vec::with_capacity(KNOWN_PROCS.len() * 4),
        }
    }

    fn handle_node(
        &mut self,
        node: WalkerNode<'a, &'a sysinfo::Process, Process<'a>>,
    ) -> ControlFlow<Pid, ContinueFlow> {
        if cfg!(debug_assertions) {
            println!(
                "{} - {} process({}): {:?}",
                node.depth,
                node.sibling_no,
                node.inner.data().pid(),
                node.inner.data().name()
            );
        }

        let known = KNOWN_PROCS.iter().any(|&name| {
            let Some(node_name) = node.inner.data().name().to_str() else {
                return false;
            };

            name == node_name
        });

        if known {
            self.known_procs.push(node.inner.data());
        }

        ControlFlow::Continue(ContinueFlow::Forward)
    }

    fn select(&self) -> Option<&'a sysinfo::Process> {
        self.known_procs.last().copied()
    }
}

fn build_path(pid: Pid, name: &str) -> PathBuf {
    let pid = pid.as_u32();
    let filename = format!("{name}-{pid}.txt");
    LOCATIONS_PATH.join(filename)
}

async fn get_location_path() -> anyhow::Result<PathBuf> {
    let active_pid_fut = tokio::spawn(Client::get_active_async());

    let refresh = RefreshKind::nothing().with_processes(ProcessRefreshKind::nothing());
    let sys = System::new_with_specifics(refresh);
    let processes = process::build_process_tree(&sys);

    let active_pid = active_pid_fut
        .await
        .context("join failed")?
        .context("failed to get active client")?
        .context("no active client")?
        .pid as u32;
    let active_pid = Pid::from_u32(active_pid);

    let root = processes.get(&active_pid).context("process not found")?;
    let mut walker = Walker::new(root, &processes);
    let mut location_search = LocationSearch::new();
    _ = walker.bfs(|node| location_search.handle_node(node));
    let selected_proc = location_search.select();

    let Some(selected_proc) = selected_proc else {
        let fallback_path = env::home_dir().unwrap_or_else(|| PathBuf::from("/home/root/"));
        return Ok(fallback_path);
    };

    let proc_pid = selected_proc.pid();
    let proc_name = selected_proc.name().to_str().context("invalid name")?;
    let path = build_path(proc_pid, proc_name);

    Ok(path)
}

#[allow(dead_code)]
async fn get_location() -> anyhow::Result<LocationData> {
    let path = get_location_path().await?;
    let file = File::open(path).context("open location file")?;
    // Blocking executor but it's fine here
    let data: LocationData =
        serde_json::from_reader(file).context("deserialize + write to file")?;
    Ok(data)
}

fn write_location(
    name: String,
    pid: Pid,
    location: PathBuf,
    nvim_pipe: Option<Pid>,
) -> anyhow::Result<()> {
    let data = LocationData {
        location,
        nvim_pipe,
    };

    fs::create_dir_all(*LOCATIONS_PATH).context("create location dir")?;
    let path = build_path(pid, &name);
    let file = File::options()
        .write(true)
        .truncate(true)
        .create(true)
        .open(path)
        .context("open location file")?;

    // Blocking executor but it's fine here
    serde_json::to_writer(file, &data).context("serialize + parse to file")?;

    Ok(())
}

fn clear_location() -> anyhow::Result<()> {
    match fs::remove_dir_all(*LOCATIONS_PATH) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

// Using `current_thread` for faster startup time
#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();

    match opts.subcommand {
        Subcommands::Get => println!(
            "{}",
            get_location_path()
                .await
                .context("get location path")?
                .display()
        ),
        Subcommands::Write {
            name,
            pid,
            location,
            nvim_pipe,
        } => write_location(name, pid, location, nvim_pipe).context("write location")?,
        Subcommands::Clear => clear_location().context("clear location")?,
    }

    Ok(())
}
