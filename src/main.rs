use std::fs::File;
use std::io;
use std::path::PathBuf;

use anyhow::Context;
use clap::{Parser, Subcommand};
use current_location::LocationData;
use current_location::process::Pid;

#[derive(Parser)]
#[command(version)]
struct Opts {
    /// Provides active pid which skips requesting it from window manager.
    ///
    /// Use it if your window namages is not supported
    #[arg(short, long, env = "CURRENT_LOCATION_ACTIVE_PID")]
    active_pid: Option<Pid>,
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

async fn print_location(active_pid: Option<Pid>) -> anyhow::Result<()> {
    let stdout = io::stdout();
    let mut stdout_lock = stdout.lock();
    let Some(path) = current_location::search(active_pid).await? else {
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

// Using `current_thread` for faster startup time
#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();

    match opts.subcommand {
        Subcommands::Get => print_location(opts.active_pid)
            .await
            .context("get location data")?,
        Subcommands::Write {
            name,
            pid,
            location,
            nvim_pipe,
        } => current_location::write(name, pid, location, nvim_pipe).context("write location")?,
        Subcommands::Clear => current_location::clear().context("clear location")?,
    }

    Ok(())
}
