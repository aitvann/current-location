use std::{
    env,
    fs::{self, File},
    io,
    ops::ControlFlow,
    path::{Path, PathBuf},
    sync::LazyLock,
};

use anyhow::Context;
use hyprland::{data::Client, shared::HyprDataActiveOptional};
use serde::{Deserialize, Serialize};

use crate::{
    process::{Pid, Process, ProcessInfo},
    walk::{ContinueFlow, Node, Walker, WalkerNode},
};

pub mod process;
pub mod walk;

const KNOWN_PROCS: &[&str] = &["zsh", "nvim"];
const BSF_HEAP_CAPACITY: usize = 1024;
static LOCATIONS_PATH: LazyLock<&Path> = LazyLock::new(|| Path::new("/tmp/current-location/"));

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LocationData {
    location: PathBuf,
    nvim_pipe: Option<Pid>,
}

impl LocationData {
    pub fn fallback() -> Self {
        Self {
            location: env::home_dir().unwrap_or_else(|| PathBuf::from("/home/root/")),
            nvim_pipe: None,
        }
    }
}

#[derive(Clone, Debug)]
struct LocationSearch<'a> {
    known_procs: Vec<&'a ProcessInfo>,
}

impl<'a> LocationSearch<'a> {
    fn new() -> Self {
        Self {
            known_procs: Vec::with_capacity(KNOWN_PROCS.len() * 4),
        }
    }

    fn handle_node(
        &mut self,
        node: WalkerNode<'a, ProcessInfo, Process>,
    ) -> ControlFlow<Pid, ContinueFlow> {
        if cfg!(debug_assertions) {
            println!(
                "{} - {} process({}): {:?}",
                node.depth,
                node.sibling_no,
                node.inner.data().pid,
                node.inner.data().name
            );
        }

        if KNOWN_PROCS.contains(&node.inner.data().name.as_str()) {
            self.known_procs.push(node.inner.data());
        }

        ControlFlow::Continue(ContinueFlow::Forward)
    }

    fn select(&self) -> Option<&'a ProcessInfo> {
        self.known_procs.last().copied()
    }
}

fn build_path(pid: Pid, name: &str) -> PathBuf {
    let filename = format!("{name}-{pid}.txt");
    LOCATIONS_PATH.join(filename)
}

pub async fn search(active_pid: Option<Pid>) -> anyhow::Result<Option<PathBuf>> {
    let active_pid_fut = if active_pid.is_none() {
        tokio::spawn(Client::get_active_async()).into()
    } else {
        None
    };

    let processes = process::build_process_tree().context("build processes tree")?;

    let active_pid = if let Some(active_pid) = active_pid {
        active_pid
    } else {
        active_pid_fut
            .expect("fut is present if active_pid is None")
            .await
            .context("join failed")?
            .context("failed to get active client")?
            .context("no active client")?
            .pid
    };

    let root = processes.get(&active_pid).context("process not found")?;
    let mut walker = Walker::with_capacity(root, &processes, BSF_HEAP_CAPACITY);
    let mut location_search = LocationSearch::new();
    _ = walker.bfs(|node| location_search.handle_node(node));
    let selected_proc = location_search.select();

    let Some(selected_proc) = selected_proc else {
        return Ok(None);
    };

    let path = build_path(selected_proc.pid, &selected_proc.name);
    Ok(path.into())
}

#[allow(dead_code)]
pub async fn get(active_pid: Option<Pid>) -> anyhow::Result<LocationData> {
    let Some(path) = search(active_pid).await? else {
        return Ok(LocationData::fallback());
    };

    let file = File::open(path).context("open location file")?;
    // Blocking executor but it's fine here
    let data: LocationData =
        serde_json::from_reader(file).context("deserialize + write to file")?;
    Ok(data)
}

pub fn write(
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

pub fn clear() -> anyhow::Result<()> {
    match fs::remove_dir_all(*LOCATIONS_PATH) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}
