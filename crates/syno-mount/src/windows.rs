//! A mount's life on Windows: rclone mounts each folder through WinFsp, and the
//! agent (`syno-mount.exe agent`, started at sign-in through the user's Run key)
//! starts, watches and restarts those rclone processes. The app tells the agent
//! what it wants through small files in the state folder.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use crate::config::{Mount, MountStore};
use crate::paths::Paths;
use crate::rclone;
use crate::sshfs::Route;
use crate::status::{self, MountStatus, State};
use crate::system;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const DETACHED_PROCESS: u32 = 0x0000_0008;
const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE: &str = "Syno Mount";
const PROBE_TIMEOUT: Duration = Duration::from_millis(1500);

fn program() -> PathBuf {
    std::env::current_exe().unwrap_or_else(|_| "syno-mount.exe".into())
}

fn want_file(paths: &Paths, id: &str) -> PathBuf {
    paths.state.join(format!("{id}.want"))
}

fn log_file(paths: &Paths, id: &str) -> PathBuf {
    paths.state.join(format!("{id}.log"))
}

/// What the user asked for last; `None` follows "mount at sign-in".
fn wanted(paths: &Paths, mount: &Mount) -> bool {
    match fs::read_to_string(want_file(paths, &mount.id)).as_deref() {
        Ok("on") => true,
        Ok("off") => false,
        _ => mount.at_login,
    }
}

fn set_wanted(paths: &Paths, id: &str, on: bool) -> io::Result<()> {
    fs::create_dir_all(&paths.state)?;
    fs::write(want_file(paths, id), if on { "on" } else { "off" })
}

/// Windows has no owner mapping; rclone shows every file as the user's own.
pub fn write_owner_maps(_paths: &Paths, _mount: &Mount) -> io::Result<()> {
    Ok(())
}

/// The rclone command over the LAN address, as the user reads it in the app.
pub fn command_line(paths: &Paths, mount: &Mount) -> String {
    let program = system::rclone_path().unwrap_or_else(|| "rclone.exe".into());
    rclone::display(&program, &rclone::arguments(mount, paths, &mount.lan))
}

fn reg(arguments: &[&str]) -> io::Result<()> {
    let output = Command::new("reg")
        .args(arguments)
        .creation_flags(CREATE_NO_WINDOW)
        .output()?;
    if output.status.success() {
        Ok(())
    } else {
        Err(io::Error::other(
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        ))
    }
}

/// Starts the agent at sign-in while any folder should be mounted then.
fn set_autostart(enabled: bool) -> io::Result<()> {
    if enabled {
        let command = format!("\"{}\" agent", program().display());
        reg(&[
            "add", RUN_KEY, "/v", RUN_VALUE, "/t", "REG_SZ", "/d", &command, "/f",
        ])
    } else {
        // Missing already is fine.
        let _ = reg(&["delete", RUN_KEY, "/v", RUN_VALUE, "/f"]);
        Ok(())
    }
}

/// rclone checks host keys against `known_hosts` by address, so every address of
/// the NAS gets the key pinned under its alias.
fn pin_addresses(paths: &Paths, mount: &Mount) -> io::Result<()> {
    let known = syno_ssh::KnownHosts::new(paths.known_hosts());
    let Some(key) = known.get(&mount.host_key_alias) else {
        return Ok(());
    };
    for address in std::iter::once(&mount.lan).chain(mount.internet.as_ref()) {
        known
            .pin(&rclone::known_hosts_name(address), &key)
            .map_err(|error| io::Error::other(error.to_string()))?;
    }
    Ok(())
}

/// Writes what the agent needs and makes sure it runs. Does not mount; see [`start`].
pub fn apply(paths: &Paths, mount: &Mount) -> io::Result<()> {
    pin_addresses(paths, mount)?;
    let store = MountStore::load(&paths.mounts());
    set_autostart(store.all().iter().any(|mount| mount.at_login))?;
    install_agent(paths)
}

fn agent_running(paths: &Paths) -> bool {
    let Ok(file) = File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(paths.state.join("agent.lock"))
    else {
        return false;
    };
    match file.try_lock() {
        Ok(()) => {
            let _ = file.unlock();
            false
        }
        Err(_) => true,
    }
}

/// Starts the agent in the background unless it runs already.
pub fn install_agent(paths: &Paths) -> io::Result<()> {
    fs::create_dir_all(&paths.state)?;
    if agent_running(paths) {
        return Ok(());
    }
    Command::new(program())
        .arg("agent")
        .creation_flags(DETACHED_PROCESS | CREATE_NO_WINDOW)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
}

fn paths_from_env() -> Paths {
    Paths::from_env()
}

pub fn start(mount: &Mount) -> io::Result<()> {
    let paths = paths_from_env();
    set_wanted(&paths, &mount.id, true)?;
    install_agent(&paths)
}

pub fn stop(mount: &Mount) -> io::Result<()> {
    set_wanted(&paths_from_env(), &mount.id, false)
}

/// Connects again, for example after changed settings: the agent sees the new
/// settings and restarts rclone.
pub fn restart(mount: &Mount) -> io::Result<()> {
    let paths = paths_from_env();
    fs::create_dir_all(&paths.state)?;
    fs::write(paths.state.join(format!("{}.restart", mount.id)), "")?;
    install_agent(&paths)
}

/// Stops the mount and removes its files on this computer. The agent ends rclone
/// once the folder is gone from the list.
pub fn remove(paths: &Paths, mount: &Mount, store_after: &MountStore) -> io::Result<()> {
    let _ = set_wanted(paths, &mount.id, false);
    status::forget_connected(paths, &mount.id);
    // Give the agent a moment to end rclone before its files go.
    std::thread::sleep(Duration::from_secs(3));
    for file in [want_file(paths, &mount.id), log_file(paths, &mount.id)] {
        let _ = fs::remove_file(file);
    }
    let known = syno_ssh::KnownHosts::new(paths.known_hosts());
    let still_used = |address: &crate::Address| {
        store_after
            .all()
            .iter()
            .any(|other| other.lan == *address || other.internet.as_ref() == Some(address))
    };
    for address in std::iter::once(&mount.lan).chain(mount.internet.as_ref()) {
        if !still_used(address) {
            let _ = known.forget(&rclone::known_hosts_name(address));
        }
    }
    set_autostart(store_after.all().iter().any(|mount| mount.at_login))
}

fn last_lines(path: &Path, count: usize) -> Option<String> {
    let text = fs::read_to_string(path).ok()?;
    let lines: Vec<&str> = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    let start = lines.len().saturating_sub(count);
    let tail = lines[start..].join("\n");
    (!tail.is_empty()).then_some(tail)
}

pub fn status(paths: &Paths, mount: &Mount) -> MountStatus {
    let wanted = wanted(paths, mount);
    let mounted = status::is_mounted(&mount.local_path);
    let log = last_lines(&log_file(paths, &mount.id), 6);
    let failed = log.as_deref().is_some_and(|log| {
        ["ERROR", "CRITICAL", "Fatal"]
            .iter()
            .any(|word| log.contains(word))
    });
    let state = match (mounted, wanted) {
        (true, _) if status::answers(&mount.local_path, Duration::from_secs(3)) => State::Mounted,
        (true, _) => State::Stale,
        (false, true) if failed => State::Failed,
        (false, true) => State::Connecting,
        (false, false) => State::Stopped,
    };
    let connected = matches!(state, State::Mounted | State::Stale)
        .then(|| status::connected(paths, &mount.id))
        .flatten();
    let dns = connected
        .as_ref()
        .map(|note| crate::dns::resolution(&note.address.host));
    let latency_ms = connected.as_ref().and_then(|note| {
        status::reach(&note.address, PROBE_TIMEOUT).map(|time| time.as_secs_f64() * 1000.0)
    });
    MountStatus {
        id: mount.id.clone(),
        state,
        connected,
        latency_ms,
        dns,
        log: matches!(state, State::Failed | State::Connecting)
            .then_some(log)
            .flatten(),
    }
}

/// Only Linux mounts through `run-mount`; on Windows the agent starts rclone.
pub fn run_mount(_paths: &Paths, _id: &str) -> Result<(), String> {
    Err("run-mount is used on Linux only; Windows mounts through the agent".into())
}

struct Running {
    child: Child,
    route: Route,
    started: Instant,
}

fn spawn(paths: &Paths, mount: &Mount, rclone: &Path) -> Option<Running> {
    let (route, address) = status::choose(mount, PROBE_TIMEOUT)?;
    let log = File::create(log_file(paths, &mount.id)).ok()?;
    let mut arguments = rclone::arguments(mount, paths, &address);
    arguments.extend(["--log-level".into(), "NOTICE".into()]);
    let child = Command::new(rclone)
        .args(&arguments)
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::from(log))
        .spawn()
        .ok()?;
    status::note_connected(paths, &mount.id, route, &address);
    Some(Running {
        child,
        route,
        started: Instant::now(),
    })
}

/// The agent's loop: keeps one rclone per wanted folder running, restarts it when
/// it ends or hangs, and moves between the LAN and the internet address.
pub fn agent(paths: &Paths) -> ! {
    let _ = fs::create_dir_all(&paths.state);
    // One agent per user: the lock is held for the agent's lifetime.
    let lock = File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(paths.state.join("agent.lock"));
    if !matches!(lock.as_ref().map(|file| file.try_lock()), Ok(Ok(()))) {
        std::process::exit(0);
    }

    let mut running: HashMap<String, Running> = HashMap::new();
    let mut retry_after: HashMap<String, Instant> = HashMap::new();
    let mut last_round = Instant::now();
    loop {
        std::thread::sleep(Duration::from_secs(2));
        let Some(rclone) = system::rclone_path() else {
            continue;
        };
        let store = MountStore::load(&paths.mounts());
        let round = last_round.elapsed() > Duration::from_secs(30);
        if round {
            last_round = Instant::now();
        }

        // Folders that are gone or no longer wanted.
        let ids: Vec<String> = running.keys().cloned().collect();
        for id in ids {
            let keep = store.get(&id).is_some_and(|mount| wanted(paths, mount));
            if !keep && let Some(mut process) = running.remove(&id) {
                let _ = process.child.kill();
                let _ = process.child.wait();
                status::forget_connected(paths, &id);
            }
        }

        for mount in store.all() {
            if !wanted(paths, mount) {
                continue;
            }
            let restart_file = paths.state.join(format!("{}.restart", mount.id));
            let asked = fs::remove_file(&restart_file).is_ok();
            if let Some(process) = running.get_mut(&mount.id) {
                let ended = process.child.try_wait().ok().flatten().is_some();
                let settled = process.started.elapsed() > Duration::from_secs(20);
                let hanging = round
                    && settled
                    && status::is_mounted(&mount.local_path)
                    && !status::answers(&mount.local_path, Duration::from_secs(4));
                let back_on_lan = round
                    && process.route == Route::Internet
                    && status::reach(&mount.lan, PROBE_TIMEOUT).is_some();
                if ended || hanging || back_on_lan || asked {
                    if let Some(mut process) = running.remove(&mount.id) {
                        let _ = process.child.kill();
                        let _ = process.child.wait();
                    }
                    let delay = if ended { 5 } else { 0 };
                    retry_after.insert(
                        mount.id.clone(),
                        Instant::now() + Duration::from_secs(delay),
                    );
                }
                continue;
            }
            if retry_after
                .get(&mount.id)
                .is_some_and(|when| Instant::now() < *when)
            {
                continue;
            }
            match spawn(paths, mount, &rclone) {
                Some(process) => {
                    running.insert(mount.id.clone(), process);
                }
                // The NAS does not answer: try again in a while.
                None => {
                    retry_after.insert(mount.id.clone(), Instant::now() + Duration::from_secs(10));
                }
            }
        }
    }
}
