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

use crate::process::{Pid, Process, ProcessInfo};
use crate::walk::{ContinueFlow, Node, Walker, WalkerNode};

mod process;
mod walk;

const KNOWN_PROCS: &[&str] = &["zsh", "nvim"];
const BSF_HEAP_CAPACITY: usize = 1024;
static LOCATIONS_PATH: LazyLock<&Path> = LazyLock::new(|| Path::new("/tmp/current-location/"));

#[derive(Parser)]
#[command(version)]
struct Opts {
    /// Provides active pid which skips requesting it from window manager.
    ///
    /// Use it if your window namages is not supported
    #[arg(short, long, env = "CURRENT_LOCATION_ACTIVE_PID")]
    active_pid: Option<Pid>,
    /// Overrides active pid provided by window manager (one will by requested anyway)
    ///
    /// Use it for testing or banchmarking purposes
    #[arg(short, long, env = "CURRENT_LOCATION_OVERRIDE_ACTIVE_PID")]
    override_active_pid: Option<Pid>,
    #[clap(subcommand)]
    subcommand: Subcommands,
}

/// A tool that help to determine Current Working File of currently active window
#[derive(Subcommand, Clone)]
enum Subcommands {
    /// Get location of currently active window
    Get,
    /// Write location of a specific program to Location Registry
    Write {
        name: String,
        pid: Pid,
        location: PathBuf,
        nvim_pipe: Option<Pid>,
    },
    /// Clear Location Registry
    Clear,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct LocationData {
    location: PathBuf,
    nvim_pipe: Option<Pid>,
}

impl LocationData {
    fn fallback() -> Self {
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

        let known = KNOWN_PROCS
            .iter()
            .any(|&name| name == node.inner.data().name);

        if known {
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

async fn search_location(opts: &Opts) -> anyhow::Result<Option<PathBuf>> {
    let active_pid_fut = if opts.active_pid.is_none() {
        tokio::spawn(Client::get_active_async()).into()
    } else {
        None
    };

    let processes = process::build_process_tree().context("build processes tree")?;

    let mut active_pid = if let Some(active_pid) = opts.active_pid {
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

    if let Some(override_active_pid) = opts.override_active_pid {
        active_pid = override_active_pid;
    }

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

async fn print_location(opts: &Opts) -> anyhow::Result<()> {
    let stdout = io::stdout();
    let mut stdout_lock = stdout.lock();
    let Some(path) = search_location(opts).await? else {
        serde_json::to_writer(stdout_lock, &LocationData::fallback())
            .context("write fallback location data to stdout")?;

        return Ok(());
    };

    // should use `splice`
    // https://doc.rust-lang.org/std/io/fn.copy.html#platform-specific-behavior
    let mut file = File::open(path).context("open location file")?;
    io::copy(&mut file, &mut stdout_lock).context("copy location file to stdout")?;

    Ok(())
}

#[allow(dead_code)]
async fn get_location(opts: &Opts) -> anyhow::Result<LocationData> {
    let Some(path) = search_location(opts).await? else {
        return Ok(LocationData::fallback());
    };

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
        Subcommands::Get => print_location(&opts).await.context("get location data")?,
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
