#![feature(substr_range)]
#![feature(slice_range)]

use std::env;
use std::fs::{self, File};
use std::ops::ControlFlow;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::{io, sync::LazyLock};

use anyhow::{Context, anyhow};
use hyprland::{data::Client, shared::HyprDataActiveOptional};
use serde::{Deserialize, Serialize};

use crate::process::{Pid, Process, ProcessInfo};
use crate::walk::{ContinueFlow, Node, Walker, WalkerNode};

pub mod process;
pub mod tosubstr;
pub mod walk;

const KNOWN_PROCS: &[&str] = &["zsh", "nvim"];
const BFS_HEAP_CAPACITY: usize = 1024;
static LOCATIONS_PATH: LazyLock<PathBuf> =
    LazyLock::new(|| PathBuf::from(format!("/tmp/current-location-{}", nix::unistd::geteuid())));

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LocationData {
    location: PathBuf,
    nvim_pipe: Option<String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    fallback: Option<bool>,
}

impl LocationData {
    pub fn fallback() -> Self {
        Self {
            location: env::home_dir().unwrap_or_else(|| PathBuf::from("/home/root/")),
            nvim_pipe: None,
            fallback: true.into(),
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
        let Some(active_client) = active_pid_fut
            .expect("fut is present if active_pid is None")
            .await
            .context("join failed")?
            .context("failed to get active client")?
        else {
            return Ok(None);
        };

        active_client.pid
    };

    let root = processes.get(&active_pid).context("process not found")?;
    let mut walker = Walker::with_capacity(root, &processes, BFS_HEAP_CAPACITY);
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

    let file = match File::open(path) {
        Ok(file) => file,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(LocationData::fallback()),
        Err(err) => return Err(anyhow!(err).context("open location file")),
    };

    // Blocking executor but it's fine here
    let data: LocationData =
        serde_json::from_reader(file).context("deserialize + write to file")?;
    Ok(data)
}

pub fn write(
    name: String,
    pids: Vec<Pid>,
    location: PathBuf,
    nvim_pipe: Option<String>,
) -> anyhow::Result<()> {
    let data = LocationData {
        location,
        nvim_pipe,
        fallback: None,
    };

    fs::create_dir_all(LOCATIONS_PATH.as_path()).context("create location dir")?;
    fs::set_permissions(LOCATIONS_PATH.as_path(), fs::Permissions::from_mode(0o700))
        .context("set permissions for location registry")?;

    for pid in pids {
        let path = build_path(pid, &name);
        let file = File::options()
            .write(true)
            .truncate(true)
            .create(true)
            .open(path)
            .context("open location file")?;
        file.metadata()
            .context("access file's metadata")?
            .permissions()
            .set_mode(0o600);

        // Blocking executor but it's fine here
        serde_json::to_writer(file, &data).context("serialize + parse to file")?;
    }

    Ok(())
}

pub fn clear() -> anyhow::Result<()> {
    match fs::remove_dir_all(LOCATIONS_PATH.as_path()) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}
