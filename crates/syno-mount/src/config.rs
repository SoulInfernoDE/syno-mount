//! The mounts the user set up, kept as JSON. Nothing secret is stored here: the SSH
//! key lives in its own file, DSM passwords in the system keychain.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use syno_dsm::CpuKind;

/// An address the SSH service of a NAS answers on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Address {
    pub host: String,
    pub port: u16,
}

/// How eagerly directory listings are cached.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Cache {
    /// Listings are kept for 20 seconds: fast, but changes made by other
    /// computers show up a little later.
    #[default]
    Normal,
    /// No listing cache, for folders that several computers change at once.
    Fresh,
}

/// Transfer settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tuning {
    /// Processor of the NAS; decides the cipher order.
    pub cpu: CpuKind,
    /// Parallel SSH connections (sshfs 3.7 and newer); 1 turns it off.
    pub connections: u8,
    /// Compression helps on slow internet links and costs time on the LAN.
    pub compress_internet: bool,
    pub cache: Cache,
}

impl Default for Tuning {
    fn default() -> Self {
        Self {
            cpu: CpuKind::Unknown,
            connections: 4,
            compress_internet: true,
            cache: Cache::Normal,
        }
    }
}

/// One shared folder mounted on this computer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Mount {
    /// Safe for file and unit names, e.g. "nas-local-5001-media".
    pub id: String,
    /// The NAS it belongs to (`host:port` of DSM, as in the device list).
    pub device_id: String,
    pub nas_name: String,
    pub share: String,
    /// The folder on the NAS as SFTP sees it, e.g. "/media" or "/volume1/media".
    pub remote_path: String,
    pub local_path: PathBuf,
    pub account: String,
    pub lan: Address,
    /// DDNS name or public address with the forwarded port; optional.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub internet: Option<Address>,
    /// Name of the pinned host key in the app's `known_hosts`.
    pub host_key_alias: String,
    pub host_key_algorithm: String,
    pub key_path: PathBuf,
    /// The account's numeric owner and group on the NAS.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_uid: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_gid: Option<u32>,
    #[serde(default)]
    pub tuning: Tuning,
    /// Mount at every login.
    pub at_login: bool,
    /// Listed in the file manager's sidebar.
    pub bookmark: bool,
}

/// Turns any text into a name that is safe for files and systemd units.
pub fn slug(text: &str) -> String {
    let mut slug = String::new();
    for c in text.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    slug.trim_matches('-').to_owned()
}

impl Mount {
    pub fn make_id(device_id: &str, share: &str) -> String {
        format!("{}-{}", slug(device_id), slug(share))
    }

    pub fn unit_name(&self) -> String {
        format!("syno-mount-{}.service", self.id)
    }
}

/// The list of mounts in `mounts.json`.
#[derive(Debug)]
pub struct MountStore {
    path: PathBuf,
    mounts: Vec<Mount>,
}

impl MountStore {
    /// A missing or unreadable file starts an empty list.
    pub fn load(path: &Path) -> Self {
        let mounts = fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Self {
            path: path.to_owned(),
            mounts,
        }
    }

    pub fn all(&self) -> &[Mount] {
        &self.mounts
    }

    pub fn get(&self, id: &str) -> Option<&Mount> {
        self.mounts.iter().find(|mount| mount.id == id)
    }

    /// Adds or replaces a mount, keeping the order of the list.
    pub fn upsert(&mut self, mount: Mount) -> io::Result<()> {
        match self.mounts.iter_mut().find(|known| known.id == mount.id) {
            Some(known) => *known = mount,
            None => self.mounts.push(mount),
        }
        self.save()
    }

    pub fn remove(&mut self, id: &str) -> io::Result<Option<Mount>> {
        let index = self.mounts.iter().position(|mount| mount.id == id);
        let removed = index.map(|index| self.mounts.remove(index));
        self.save()?;
        Ok(removed)
    }

    /// Writes to a temporary file first, so a crash never leaves half a file.
    fn save(&self) -> io::Result<()> {
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        let temporary = self.path.with_extension("json.tmp");
        fs::write(&temporary, serde_json::to_vec_pretty(&self.mounts)?)?;
        fs::rename(&temporary, &self.path)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn sample(root: &Path) -> Mount {
        Mount {
            id: Mount::make_id("nas.local:5001", "Media"),
            device_id: "nas.local:5001".into(),
            nas_name: "nas".into(),
            share: "Media".into(),
            remote_path: "/Media".into(),
            local_path: root.join("NAS/nas/Media"),
            account: "anna".into(),
            lan: Address {
                host: "192.168.1.20".into(),
                port: 22,
            },
            internet: Some(Address {
                host: "nas.example.org".into(),
                port: 2222,
            }),
            host_key_alias: "syno-mount.nas.local_5001".into(),
            host_key_algorithm: "ssh-ed25519".into(),
            key_path: root.join("keys/nas"),
            remote_uid: Some(1026),
            remote_gid: Some(100),
            tuning: Tuning::default(),
            at_login: true,
            bookmark: true,
        }
    }

    #[test]
    fn slugs_are_safe_and_stable() {
        assert_eq!(slug("nas.local:5001"), "nas-local-5001");
        assert_eq!(slug("  Fotos & Videos "), "fotos-videos");
        assert_eq!(Mount::make_id("[fe80::1]:5001", "home"), "fe80-1-5001-home");
    }

    #[test]
    fn keeps_mounts_across_loads() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mounts.json");
        let mut store = MountStore::load(&path);
        let mount = sample(dir.path());
        store.upsert(mount.clone()).unwrap();
        let mut changed = mount.clone();
        changed.at_login = false;
        store.upsert(changed.clone()).unwrap();

        let reloaded = MountStore::load(&path);
        assert_eq!(reloaded.all(), [changed]);
        assert_eq!(mount.unit_name(), "syno-mount-nas-local-5001-media.service");
        let mut reloaded = reloaded;
        assert!(reloaded.remove(&mount.id).unwrap().is_some());
        assert!(MountStore::load(&path).all().is_empty());
    }
}
