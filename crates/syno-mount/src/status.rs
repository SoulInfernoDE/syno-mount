//! Is a mount there, does it answer, and which address does it use.

use std::fs;
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::config::{Address, Mount};
use crate::paths::Paths;
use crate::sshfs::Route;

/// Undoes the octal escapes of `/proc/self/mountinfo` ("\040" is a space).
fn unescape(field: &str) -> String {
    let bytes = field.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\'
            && let Some(digits) = bytes.get(i + 1..i + 4)
            && let Ok(value) = u8::from_str_radix(&String::from_utf8_lossy(digits), 8)
        {
            out.push(value);
            i += 4;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Mount points of FUSE sshfs mounts with their source ("syno-mount:<id>" for ours).
pub fn sshfs_mounts(mountinfo: &str) -> Vec<(PathBuf, String)> {
    mountinfo
        .lines()
        .filter_map(|line| {
            let (left, right) = line.split_once(" - ")?;
            let point = left.split(' ').nth(4)?;
            let mut right = right.split(' ');
            let kind = right.next()?;
            let source = right.next()?;
            (kind == "fuse.sshfs").then(|| (PathBuf::from(unescape(point)), unescape(source)))
        })
        .collect()
}

pub fn is_mounted(path: &Path) -> bool {
    let info = fs::read_to_string("/proc/self/mountinfo").unwrap_or_default();
    sshfs_mounts(&info).iter().any(|(point, _)| point == path)
}

/// Whether the mount answers within `timeout`. A mount whose connection is gone
/// hangs on every access, so the check runs on its own thread.
pub fn answers(path: &Path, timeout: Duration) -> bool {
    let (sender, receiver) = mpsc::channel();
    let path = path.to_owned();
    std::thread::spawn(move || {
        let _ = sender.send(fs::read_dir(&path).is_ok());
    });
    receiver.recv_timeout(timeout).unwrap_or(false)
}

/// Time to open a TCP connection, or `None` when nothing answers within `timeout`.
pub fn reach(address: &Address, timeout: Duration) -> Option<Duration> {
    let targets = (address.host.as_str(), address.port)
        .to_socket_addrs()
        .ok()?;
    for target in targets {
        let started = Instant::now();
        if TcpStream::connect_timeout(&target, timeout).is_ok() {
            return Some(started.elapsed());
        }
    }
    None
}

/// The address to use now: the LAN one when it answers, else the internet one.
pub fn choose(mount: &Mount, timeout: Duration) -> Option<(Route, Address)> {
    if reach(&mount.lan, timeout).is_some() {
        return Some((Route::Lan, mount.lan.clone()));
    }
    let internet = mount.internet.as_ref()?;
    reach(internet, timeout).map(|_| (Route::Internet, internet.clone()))
}

/// What `run-mount` noted when it started sshfs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Connected {
    pub route: Route,
    pub address: Address,
    /// Seconds since 1970.
    pub since: u64,
}

fn state_file(paths: &Paths, id: &str) -> PathBuf {
    paths.state.join(format!("{id}.json"))
}

pub fn note_connected(paths: &Paths, id: &str, route: Route, address: &Address) {
    let since = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    let note = Connected {
        route,
        address: address.clone(),
        since,
    };
    if fs::create_dir_all(&paths.state).is_ok()
        && let Ok(json) = serde_json::to_vec(&note)
    {
        let _ = fs::write(state_file(paths, id), json);
    }
}

pub fn connected(paths: &Paths, id: &str) -> Option<Connected> {
    let bytes = fs::read(state_file(paths, id)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

pub fn forget_connected(paths: &Paths, id: &str) {
    let _ = fs::remove_file(state_file(paths, id));
}

#[cfg(test)]
mod tests {
    use super::*;

    const MOUNTINFO: &str = "\
22 1 0:21 / / rw,relatime shared:1 - btrfs /dev/nvme0n1p2 rw
95 22 0:52 / /home/anna/NAS/nas/My\\040Media rw,nosuid,nodev,relatime shared:50 - fuse.sshfs syno-mount:nas-local-5001-my-media rw,user_id=1000,group_id=1000
96 22 0:53 / /mnt/other rw,nosuid,nodev - fuse.sshfs anna@host:/x rw
97 22 0:54 / /mnt/smb rw - cifs //nas/x rw
";

    #[test]
    fn finds_sshfs_mounts_with_spaces() {
        let mounts = sshfs_mounts(MOUNTINFO);
        assert_eq!(
            mounts,
            [
                (
                    PathBuf::from("/home/anna/NAS/nas/My Media"),
                    "syno-mount:nas-local-5001-my-media".to_owned()
                ),
                (PathBuf::from("/mnt/other"), "anna@host:/x".to_owned()),
            ]
        );
    }

    #[test]
    fn a_closed_port_is_not_reachable() {
        // Port 9 (discard) is closed on test machines.
        let address = Address {
            host: "127.0.0.1".into(),
            port: 9,
        };
        assert_eq!(reach(&address, Duration::from_millis(300)), None);
    }

    #[test]
    fn remembers_the_route() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::with_home(dir.path().to_owned());
        let paths = Paths {
            state: dir.path().join("state"),
            ..paths
        };
        let address = Address {
            host: "nas".into(),
            port: 22,
        };
        note_connected(&paths, "m", Route::Lan, &address);
        let note = connected(&paths, "m").unwrap();
        assert_eq!((note.route, note.address), (Route::Lan, address));
        forget_connected(&paths, "m");
        assert_eq!(connected(&paths, "m"), None);
    }
}
