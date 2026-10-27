//! A mount's life on this computer: files and units written, started, watched,
//! and everything removed again.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::config::{Mount, MountStore};
use crate::paths::Paths;
use crate::sshfs::{self, Route};
use crate::status::{self, Connected};
use crate::{bookmarks, systemd};

/// How long `run-mount` waits for the NAS to answer, e.g. while the network comes
/// up after login. systemd tries again afterwards.
const WAIT_FOR_NAS: Duration = Duration::from_secs(30);
const PROBE_TIMEOUT: Duration = Duration::from_millis(1500);

/// Name of an entry in `/etc/passwd` or `/etc/group` with the given number.
fn name_for(file: &str, id: u32) -> Option<String> {
    fs::read_to_string(file).ok()?.lines().find_map(|line| {
        let mut fields = line.split(':');
        let name = fields.next()?;
        let number: u32 = fields.nth(1)?.parse().ok()?;
        (number == id).then(|| name.to_owned())
    })
}

/// Real user and group ID of this process, from `/proc`.
fn own_ids() -> Option<(u32, u32)> {
    let status = fs::read_to_string("/proc/self/status").ok()?;
    let field = |key: &str| -> Option<u32> {
        status
            .lines()
            .find(|line| line.starts_with(key))?
            .split_whitespace()
            .nth(1)?
            .parse()
            .ok()
    };
    Some((field("Uid:")?, field("Gid:")?))
}

fn write_private(path: &Path, text: &str) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(text.as_bytes())
}

/// The files sshfs reads to show the account's files on the NAS as one's own.
pub fn write_owner_maps(paths: &Paths, mount: &Mount) -> io::Result<()> {
    let Some((uid, gid)) = own_ids() else {
        return Ok(());
    };
    if let (Some(remote), Some(user)) = (mount.remote_uid, name_for("/etc/passwd", uid)) {
        write_private(
            &paths.owner_map(&mount.id, "uid"),
            &format!("{user}:{remote}\n"),
        )?;
    }
    if let (Some(remote), Some(group)) = (mount.remote_gid, name_for("/etc/group", gid)) {
        write_private(
            &paths.owner_map(&mount.id, "gid"),
            &format!("{group}:{remote}\n"),
        )?;
    }
    Ok(())
}

/// The sshfs command over the LAN address, as the user reads it in the app.
pub fn command_line(paths: &Paths, mount: &Mount) -> String {
    let version = sshfs::installed_version().unwrap_or((3, 7, 0));
    sshfs::arguments(mount, paths, &mount.lan, Route::Lan, version)
        .map(|arguments| sshfs::display(&arguments))
        .unwrap_or_else(|error| error)
}

/// Writes everything a mount needs and sets it to start at login or not.
/// Does not mount; see [`start`].
pub fn apply(paths: &Paths, mount: &Mount) -> io::Result<()> {
    fs::create_dir_all(&mount.local_path)?;
    write_owner_maps(paths, mount)?;
    let program = systemd::program();
    let unit = systemd::mount_unit(mount, &program, &command_line(paths, mount));
    systemd::install(paths, &mount.unit_name(), &unit)?;
    systemd::set_enabled(&mount.unit_name(), mount.at_login)?;
    let label = format!("{} ({})", mount.share, mount.nas_name);
    if mount.bookmark {
        bookmarks::add(&paths.bookmarks(), &mount.local_path, &label)?;
    } else {
        bookmarks::remove(&paths.bookmarks(), &mount.local_path)?;
    }
    install_agent(paths)
}

/// The agent runs while there is any mount.
pub fn install_agent(paths: &Paths) -> io::Result<()> {
    let program = systemd::program();
    systemd::install(paths, systemd::AGENT_UNIT, &systemd::agent_unit(&program))?;
    systemd::set_enabled(systemd::AGENT_UNIT, true)?;
    systemd::start(systemd::AGENT_UNIT)
}

pub fn start(mount: &Mount) -> io::Result<()> {
    systemd::start(&mount.unit_name())
}

pub fn stop(mount: &Mount) -> io::Result<()> {
    systemd::stop(&mount.unit_name())
}

/// Stops the mount and removes its unit, bookmark and files on this computer. The
/// key on the NAS is the caller's business (it needs a connection).
pub fn remove(paths: &Paths, mount: &Mount, store_after: &MountStore) -> io::Result<()> {
    systemd::uninstall(paths, &mount.unit_name())?;
    let _ = Command::new("fusermount3")
        .args(["-uz"])
        .arg(&mount.local_path)
        .output();
    bookmarks::remove(&paths.bookmarks(), &mount.local_path)?;
    for kind in ["uid", "gid"] {
        let _ = fs::remove_file(paths.owner_map(&mount.id, kind));
    }
    status::forget_connected(paths, &mount.id);
    // An empty mount folder goes too; one with files in it stays.
    let _ = fs::remove_dir(&mount.local_path);
    if store_after.all().is_empty() {
        systemd::uninstall(paths, systemd::AGENT_UNIT)?;
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum State {
    /// Mounted and answering.
    Mounted,
    /// Mounted, but the connection hangs.
    Stale,
    /// The service is waiting for the NAS or connecting.
    Connecting,
    Stopped,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MountStatus {
    pub id: String,
    pub state: State,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connected: Option<Connected>,
    /// Round trip of a TCP connect to the address in use.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<f64>,
    /// The service's last words when it failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log: Option<String>,
}

pub fn status(paths: &Paths, mount: &Mount) -> MountStatus {
    let unit = systemd::state(&mount.unit_name());
    let mounted = status::is_mounted(&mount.local_path);
    let state = match (unit.as_str(), mounted) {
        (_, true) if status::answers(&mount.local_path, Duration::from_secs(3)) => State::Mounted,
        (_, true) => State::Stale,
        ("active" | "activating" | "reloading", false) => State::Connecting,
        ("failed", false) => State::Failed,
        _ => State::Stopped,
    };
    let connected = matches!(state, State::Mounted | State::Stale)
        .then(|| status::connected(paths, &mount.id))
        .flatten();
    let latency_ms = connected.as_ref().and_then(|note| {
        status::reach(&note.address, PROBE_TIMEOUT).map(|time| time.as_secs_f64() * 1000.0)
    });
    let log = matches!(state, State::Failed | State::Connecting)
        .then(|| systemd::last_log(&mount.unit_name(), 6))
        .filter(|log| !log.is_empty());
    MountStatus {
        id: mount.id.clone(),
        state,
        connected,
        latency_ms,
        log,
    }
}

/// What systemd runs: picks the address, notes it, and becomes sshfs.
pub fn run_mount(paths: &Paths, id: &str) -> Result<(), String> {
    let store = MountStore::load(&paths.mounts());
    let mount = store
        .get(id)
        .ok_or_else(|| format!("no mount with the ID {id}"))?
        .clone();
    let started = Instant::now();
    let (route, address) = loop {
        if let Some(choice) = status::choose(&mount, PROBE_TIMEOUT) {
            break choice;
        }
        if started.elapsed() > WAIT_FOR_NAS {
            return Err(format!("{} does not answer on any address", mount.nas_name));
        }
        std::thread::sleep(Duration::from_secs(3));
    };
    fs::create_dir_all(&mount.local_path).map_err(|e| e.to_string())?;
    let version = sshfs::installed_version().ok_or("sshfs is not installed")?;
    let arguments = sshfs::arguments(&mount, paths, &address, route, version)?;
    eprintln!("{}", sshfs::display(&arguments));
    status::note_connected(paths, &mount.id, route, &address);
    // Only returns when sshfs could not be started.
    let error = Command::new("sshfs").args(&arguments).exec();
    Err(format!("cannot start sshfs: {error}"))
}

/// Seconds since boot including suspend; `Instant` leaves suspend out.
fn uptime() -> Option<f64> {
    fs::read_to_string("/proc/uptime")
        .ok()?
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

/// Changes when the routes change: a different network, VPN up or down.
fn network_fingerprint() -> String {
    fs::read_to_string("/proc/net/route").unwrap_or_default()
}

/// Puts a mount right after an event: releases a hanging one, and moves to the
/// LAN or the internet address when the other one is the right one now.
fn heal(paths: &Paths, mount: &Mount, event: bool) {
    let unit = mount.unit_name();
    let unit_state = systemd::state(&unit);
    let running = matches!(unit_state.as_str(), "active" | "activating");
    if !running {
        // A failed mount that should be there gets another try after an event.
        if event && mount.at_login && unit_state == "failed" {
            let _ = systemd::restart(&unit);
        }
        return;
    }
    if !status::is_mounted(&mount.local_path) {
        return; // still connecting
    }
    let restart = if !status::answers(&mount.local_path, Duration::from_secs(4)) {
        eprintln!("{}: not answering, mounting again", mount.id);
        true
    } else if event {
        let lan_up = status::reach(&mount.lan, PROBE_TIMEOUT).is_some();
        match status::connected(paths, &mount.id).map(|note| note.route) {
            Some(Route::Internet) if lan_up => {
                eprintln!("{}: back on the LAN", mount.id);
                true
            }
            Some(Route::Lan) if !lan_up => {
                eprintln!("{}: LAN address gone", mount.id);
                mount.internet.is_some()
            }
            _ => false,
        }
    } else {
        false
    };
    if restart {
        let _ = Command::new("fusermount3")
            .arg("-uz")
            .arg(&mount.local_path)
            .output();
        let _ = systemd::restart(&unit);
    }
}

/// The agent's loop: every few seconds it looks for suspend and network changes,
/// and every half minute it checks that the mounts answer.
pub fn agent(paths: &Paths) -> ! {
    let tick = Duration::from_secs(5);
    let mut last_tick = Instant::now();
    let mut last_uptime = uptime();
    let mut last_network = network_fingerprint();
    let mut last_round = Instant::now();
    loop {
        std::thread::sleep(tick);
        let now_uptime = uptime();
        let slept = match (last_uptime, now_uptime) {
            (Some(before), Some(now)) => now - before - last_tick.elapsed().as_secs_f64() > 3.0,
            _ => false,
        };
        let network = network_fingerprint();
        let changed = network != last_network;
        last_tick = Instant::now();
        last_uptime = now_uptime;
        last_network = network;

        let event = slept || changed;
        if event || last_round.elapsed() > Duration::from_secs(30) {
            if event {
                // Give the network a moment to settle after resume or a switch.
                std::thread::sleep(Duration::from_secs(3));
                eprintln!(
                    "{}",
                    if slept {
                        "resumed from suspend"
                    } else {
                        "network changed"
                    }
                );
            }
            let store = MountStore::load(&paths.mounts());
            for mount in store.all() {
                heal(paths, mount, event);
            }
            last_round = Instant::now();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_names_for_ids() {
        assert_eq!(name_for("/etc/passwd", 0).as_deref(), Some("root"));
        assert_eq!(name_for("/etc/group", 0).as_deref(), Some("root"));
        assert!(own_ids().is_some());
        assert!(uptime().unwrap() > 0.0);
    }
}
