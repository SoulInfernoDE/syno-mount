//! The app's own key files and `known_hosts`.

use std::fs;
use std::io::{self, Read, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use russh::keys::ssh_key::LineEnding;
use russh::keys::ssh_key::private::Ed25519Keypair;
use russh::keys::{PrivateKey, PublicKey};

use crate::{Error, HostKey};

/// An ed25519 key pair stored as `<name>` (private, mode 600) and `<name>.pub`.
#[derive(Debug, Clone)]
pub struct KeyPair {
    pub private_path: PathBuf,
    pub key: PrivateKey,
}

impl KeyPair {
    /// Loads the key at `path`, or creates it with `comment` when there is none.
    pub fn load_or_create(path: &Path, comment: &str) -> Result<Self, Error> {
        if path.exists() {
            let pem = fs::read_to_string(path)?;
            let key =
                PrivateKey::from_openssh(pem).map_err(|error| Error::Io(error.to_string()))?;
            return Ok(Self {
                private_path: path.to_owned(),
                key,
            });
        }
        let mut seed = [0u8; 32];
        fs::File::open("/dev/urandom")?.read_exact(&mut seed)?;
        let mut key = PrivateKey::from(Ed25519Keypair::from_seed(&seed));
        key.set_comment(comment);
        let pem = key
            .to_openssh(LineEnding::LF)
            .map_err(|error| Error::Io(error.to_string()))?;
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
            fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
        }
        write_private(path, pem.as_bytes())?;
        let public = key
            .public_key()
            .to_openssh()
            .map_err(|error| Error::Io(error.to_string()))?;
        fs::write(path.with_extension("pub"), format!("{public}\n"))?;
        Ok(Self {
            private_path: path.to_owned(),
            key,
        })
    }

    /// The line for `authorized_keys`, ending in the key's comment.
    pub fn authorized_line(&self) -> String {
        self.key.public_key().to_openssh().unwrap_or_default()
    }

    pub fn public(&self) -> &PublicKey {
        self.key.public_key()
    }
}

fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let temporary = path.with_extension("tmp");
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    fs::rename(&temporary, path)
}

/// The name a NAS goes by in `known_hosts`, the same for its LAN and internet
/// address (ssh's `HostKeyAlias`).
pub fn host_alias(device_id: &str) -> String {
    let slug: String = device_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!("syno-mount.{slug}")
}

/// The app's own `known_hosts` file: one line per NAS, keyed by [`host_alias`].
#[derive(Debug, Clone)]
pub struct KnownHosts {
    pub path: PathBuf,
}

impl KnownHosts {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    fn lines(&self) -> Vec<String> {
        fs::read_to_string(&self.path)
            .map(|text| text.lines().map(str::to_owned).collect())
            .unwrap_or_default()
    }

    /// The pinned key of `alias`.
    pub fn get(&self, alias: &str) -> Option<HostKey> {
        self.lines().iter().find_map(|line| {
            let (name, key) = line.split_once(' ')?;
            if name != alias {
                return None;
            }
            PublicKey::from_openssh(key)
                .ok()
                .map(|key| HostKey::from_public(&key))
        })
    }

    /// Pins `key` for `alias`, replacing an earlier key.
    pub fn pin(&self, alias: &str, key: &HostKey) -> Result<(), Error> {
        let mut lines = self.without(alias);
        lines.push(format!("{alias} {}", key.openssh));
        self.write(&lines)
    }

    pub fn forget(&self, alias: &str) -> Result<(), Error> {
        let lines = self.without(alias);
        self.write(&lines)
    }

    fn without(&self, alias: &str) -> Vec<String> {
        self.lines()
            .into_iter()
            .filter(|line| line.split_once(' ').map(|(name, _)| name) != Some(alias))
            .collect()
    }

    fn write(&self, lines: &[String]) -> Result<(), Error> {
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        let mut text = lines.join("\n");
        if !text.is_empty() {
            text.push('\n');
        }
        write_private(&self.path, text.as_bytes())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_a_key_once_and_loads_it_again() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("keys/nas");
        let created = KeyPair::load_or_create(&path, "syno-mount:test").unwrap();
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        assert!(created.authorized_line().starts_with("ssh-ed25519 "));
        assert!(created.authorized_line().ends_with(" syno-mount:test"));
        let loaded = KeyPair::load_or_create(&path, "other").unwrap();
        assert_eq!(loaded.authorized_line(), created.authorized_line());
        let public = fs::read_to_string(path.with_extension("pub")).unwrap();
        assert_eq!(public.trim_end(), created.authorized_line());
    }

    #[test]
    fn pins_replaces_and_forgets_host_keys() {
        let dir = tempfile::tempdir().unwrap();
        let known = KnownHosts::new(dir.path().join("known_hosts"));
        let first = KeyPair::load_or_create(&dir.path().join("a"), "").unwrap();
        let second = KeyPair::load_or_create(&dir.path().join("b"), "").unwrap();
        let key_a = HostKey::from_public(first.public());
        let key_b = HostKey::from_public(second.public());

        known.pin("syno-mount.one", &key_a).unwrap();
        known.pin("syno-mount.two", &key_a).unwrap();
        known.pin("syno-mount.one", &key_b).unwrap();
        assert_eq!(known.get("syno-mount.one"), Some(key_b.clone()));
        assert_eq!(known.get("syno-mount.two"), Some(key_a));
        known.forget("syno-mount.two").unwrap();
        assert_eq!(known.get("syno-mount.two"), None);
        assert!(key_b.fingerprint.starts_with("SHA256:"));
        assert_eq!(key_b.algorithm, "ssh-ed25519");
    }

    #[test]
    fn aliases_are_safe_names() {
        assert_eq!(host_alias("nas.local:5001"), "syno-mount.nas.local_5001");
        assert_eq!(host_alias("[fe80::1]:5001"), "syno-mount._fe80__1__5001");
    }
}
