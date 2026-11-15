//! What Syno Mount does on the NAS over SFTP: install and remove its key, and find
//! out how the account sees the shared folders.

use russh_sftp::protocol::{FileAttributes, OpenFlags};
use serde::Serialize;
use tokio::io::AsyncWriteExt;

use crate::{Error, Sftp};

/// Every key line Syno Mount adds carries this in its comment, so it can remove
/// exactly its own lines and nothing else.
pub const KEY_MARKER: &str = "syno-mount:";

/// How the account sees the NAS over SFTP.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Layout {
    /// The folder SFTP starts in, e.g. "/var/services/homes/anna" or "/home".
    pub home: String,
    /// Names in "/": shared folders for accounts DSM keeps in a chroot, the system
    /// folders for administrators.
    pub root: Vec<String>,
}

/// System folders that only an account outside DSM's chroot sees in "/".
const SYSTEM_FOLDERS: &[&str] = &["bin", "etc", "usr", "var"];

impl Layout {
    /// DSM keeps accounts outside the administrators in a chroot whose "/" holds
    /// only their shared folders.
    pub fn chrooted(&self) -> bool {
        !SYSTEM_FOLDERS
            .iter()
            .any(|folder| self.root.iter().any(|name| name == folder))
    }

    /// The path of a shared folder over SFTP: "/<share>" when the account sees the
    /// shares at the top, otherwise the volume path DSM reported ("/volume1/<share>").
    pub fn share_path(&self, share: &str, volume_path: Option<&str>) -> String {
        if self.root.iter().any(|name| name == share) {
            return format!("/{share}");
        }
        match volume_path {
            Some(path) => path.to_owned(),
            None => format!("/{share}"),
        }
    }
}

/// The permissions of the paths OpenSSH checks before it takes a key from
/// `authorized_keys` (`StrictModes`); `None` where a path is missing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyModes {
    pub home: Option<u32>,
    pub ssh: Option<u32>,
    pub keys: Option<u32>,
}

impl KeyModes {
    /// Group or others may write one of the paths, so sshd ignores the key. DSM
    /// shows home folders with Windows ACLs this way, often as 777.
    pub fn too_open(&self) -> bool {
        [self.home, self.ssh, self.keys]
            .into_iter()
            .flatten()
            .any(|mode| mode & 0o022 != 0)
    }

    /// "home 777, .ssh 777, authorized_keys 777", for the error detail.
    pub fn describe(&self) -> String {
        let show = |mode: Option<u32>| match mode {
            Some(mode) => format!("{:03o}", mode & 0o7777),
            None => "missing".into(),
        };
        format!(
            "home {}, .ssh {}, authorized_keys {}",
            show(self.home),
            show(self.ssh),
            show(self.keys)
        )
    }

    /// Why a key login failed right after the key was installed.
    pub fn refusal(&self) -> Error {
        Error::KeyRefused {
            modes: self.describe(),
            too_open: self.too_open(),
        }
    }
}

fn step_error(what: &str, error: russh_sftp::client::error::Error) -> Error {
    Error::Sftp(format!("{what}: {error}"))
}

fn mode(permissions: u32) -> FileAttributes {
    FileAttributes {
        permissions: Some(permissions),
        ..FileAttributes::default()
    }
}

/// Adds `line` to `text` unless it is there already; ends with a newline.
pub(crate) fn with_key(text: &str, line: &str) -> String {
    let mut lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    if !lines.iter().any(|existing| existing.trim() == line.trim()) {
        lines.push(line.trim());
    }
    lines.join("\n") + "\n"
}

/// The key itself in an `authorized_keys` line: the base64 after the type. Options
/// in front (`restrict,… ssh-ed25519 AAAA…`) are skipped.
fn key_blob(line: &str) -> Option<&str> {
    let fields: Vec<&str> = line.split_whitespace().collect();
    let kind = fields
        .iter()
        .position(|field| field.starts_with("ssh-") || field.starts_with("ecdsa-"))?;
    fields.get(kind + 1).copied()
}

/// Removes the lines of the key in `line` from `text`. Matched by the key itself,
/// not the comment: two computers with the same name keep their own keys.
pub(crate) fn without_key(text: &str, line: &str) -> String {
    let Some(ours) = key_blob(line) else {
        return text.to_owned();
    };
    let kept: Vec<&str> = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter(|line| key_blob(line) != Some(ours))
        .collect();
    if kept.is_empty() {
        String::new()
    } else {
        kept.join("\n") + "\n"
    }
}

impl Sftp {
    /// Writes `bytes` to `path`, creating the file. (`SftpSession::write` only
    /// writes files that exist.)
    async fn write_file(
        &self,
        path: &str,
        bytes: &[u8],
    ) -> Result<(), russh_sftp::client::error::Error> {
        let flags = OpenFlags::CREATE | OpenFlags::TRUNCATE | OpenFlags::WRITE;
        let mut file = self.session.open_with_flags(path, flags).await?;
        let io = |error: std::io::Error| russh_sftp::client::error::Error::IO(error.to_string());
        file.write_all(bytes).await.map_err(io)?;
        file.shutdown().await.map_err(io)
    }

    /// The account's home folder. DSM starts accounts outside the administrators in
    /// a chroot at "/", where their home is the "home" folder.
    async fn home(&self) -> Result<String, Error> {
        let start = self.session.canonicalize(".").await?;
        if start == "/" && self.session.try_exists("/home").await? {
            return Ok("/home".into());
        }
        Ok(start)
    }

    pub async fn layout(&self) -> Result<Layout, Error> {
        let home = self.home().await?;
        let mut root: Vec<String> = self
            .session
            .read_dir("/")
            .await?
            .map(|entry| entry.file_name())
            .filter(|name| name != "." && name != "..")
            .collect();
        root.sort();
        Ok(Layout { home, root })
    }

    /// Numeric owner and group of the account on the NAS, read from its home.
    /// sshfs maps them to the local user, so own files show as one's own.
    pub async fn owner(&self) -> Result<(Option<u32>, Option<u32>), Error> {
        let home = self.home().await?;
        let attributes = self.session.metadata(&home).await?;
        Ok((attributes.uid, attributes.gid))
    }

    /// Shared folders as SFTP shows them, for accounts that may not use the DSM Web
    /// API: (name, SFTP path). In the chroot they are the folders in "/"; for
    /// administrators the folders on each "/volumeN" that DSM does not hide.
    pub async fn shares(&self) -> Result<Vec<(String, String)>, Error> {
        let layout = self.layout().await?;
        let mut shares = Vec::new();
        if layout.chrooted() {
            for name in &layout.root {
                shares.push((name.clone(), format!("/{name}")));
            }
        } else {
            for volume in layout.root.iter().filter(|name| name.starts_with("volume")) {
                let entries = match self.session.read_dir(format!("/{volume}")).await {
                    Ok(entries) => entries,
                    Err(_) => continue,
                };
                for entry in entries {
                    let name = entry.file_name();
                    let hidden = name.starts_with(['@', '#', '.']) || name == "lost+found";
                    if entry.file_type().is_dir() && !hidden {
                        shares.push((name.clone(), format!("/{volume}/{name}")));
                    }
                }
            }
        }
        shares.sort_by_key(|(name, _)| name.to_lowercase());
        Ok(shares)
    }

    /// Adds `line` to `~/.ssh/authorized_keys` and sets the modes sshd insists on:
    /// `.ssh` 700, `authorized_keys` 600, a home that only its owner may write.
    pub async fn install_key(&self, line: &str) -> Result<(), Error> {
        let home = self.home().await?;
        let ssh = format!("{}/.ssh", home.trim_end_matches('/'));
        let file = format!("{ssh}/authorized_keys");
        let step = |what: &'static str| move |error| step_error(what, error);

        // sshd ignores keys when the home can be written by others. In DSM's chroot
        // the home shows 777 from its ACL and refuses the change, while the real
        // folder is 711; so this is only an attempt, and the key login checks it.
        let home_mode = self
            .session
            .metadata(&home)
            .await
            .map_err(step("read home"))?
            .permissions
            .unwrap_or(0);
        if home_mode & 0o022 != 0 {
            let _ = self
                .session
                .set_metadata(&home, mode(home_mode & 0o7755))
                .await;
        }
        if !self
            .session
            .try_exists(&ssh)
            .await
            .map_err(step("look for .ssh"))?
        {
            self.session
                .create_dir(&ssh)
                .await
                .map_err(step("create .ssh"))?;
        }
        self.session
            .set_metadata(&ssh, mode(0o700))
            .await
            .map_err(step("protect .ssh"))?;

        let current = if self
            .session
            .try_exists(&file)
            .await
            .map_err(step("look for keys"))?
        {
            let bytes = self.session.read(&file).await.map_err(step("read keys"))?;
            String::from_utf8_lossy(&bytes).into_owned()
        } else {
            String::new()
        };
        let next = with_key(&current, line);
        if next != current {
            self.write_file(&file, next.as_bytes())
                .await
                .map_err(step("write keys"))?;
        }
        self.session
            .set_metadata(&file, mode(0o600))
            .await
            .map_err(step("protect keys"))?;
        Ok(())
    }

    /// The modes of the home, `.ssh` and `authorized_keys` as SFTP shows them.
    pub async fn key_modes(&self) -> Result<KeyModes, Error> {
        let home = self.home().await?;
        let ssh = format!("{}/.ssh", home.trim_end_matches('/'));
        let file = format!("{ssh}/authorized_keys");
        let mode = async |path: &str| {
            self.session
                .metadata(path)
                .await
                .ok()
                .and_then(|attributes| attributes.permissions)
        };
        Ok(KeyModes {
            home: mode(&home).await,
            ssh: mode(&ssh).await,
            keys: mode(&file).await,
        })
    }

    /// Removes the key of `line` (an `authorized_keys` line) from
    /// `~/.ssh/authorized_keys`.
    pub async fn remove_key(&self, line: &str) -> Result<(), Error> {
        let home = self.home().await?;
        let file = format!("{}/.ssh/authorized_keys", home.trim_end_matches('/'));
        if !self.session.try_exists(&file).await? {
            return Ok(());
        }
        let current = String::from_utf8_lossy(&self.session.read(&file).await?).into_owned();
        let next = without_key(&current, line);
        if next != current {
            self.write_file(&file, next.as_bytes()).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OURS: &str = "ssh-ed25519 AAAAours syno-mount:anna@laptop";
    const THEIRS: &str = "ssh-rsa AAAAtheirs anna@desktop";

    #[test]
    fn adds_a_key_once() {
        let once = with_key(&format!("{THEIRS}\n\n"), OURS);
        assert_eq!(once, format!("{THEIRS}\n{OURS}\n"));
        assert_eq!(with_key(&once, OURS), once);
        assert_eq!(with_key("", OURS), format!("{OURS}\n"));
    }

    #[test]
    fn removes_only_its_own_key() {
        let text = format!("{THEIRS}\n{OURS}\n");
        assert_eq!(without_key(&text, OURS), format!("{THEIRS}\n"));
        assert_eq!(without_key(&format!("{OURS}\n"), OURS), "");
        // Another computer with the same name and account keeps its key.
        let twin = "ssh-ed25519 AAAAtwin syno-mount:anna@laptop";
        assert_eq!(
            without_key(&format!("{twin}\n{OURS}\n"), OURS),
            format!("{twin}\n")
        );
        // Options in front of the key do not hide it.
        let restricted = format!("restrict {OURS}");
        assert_eq!(without_key(&format!("{restricted}\n"), OURS), "");
        assert_eq!(without_key(&text, "not a key"), text);
    }

    #[test]
    fn finds_modes_that_sshd_rejects() {
        // As DSM 7.4.1 shows a home folder whose ACL lets everyone write, over SFTP
        // (with the directory and file type bits).
        let acl = KeyModes {
            home: Some(0o40777),
            ssh: Some(0o40777),
            keys: Some(0o100777),
        };
        assert!(acl.too_open());
        assert_eq!(acl.describe(), "home 777, .ssh 777, authorized_keys 777");
        assert_eq!(acl.refusal().key(), "key-refused-acl-home");

        // What the commands of the administrator area leave behind.
        let fixed = KeyModes {
            home: Some(0o40755),
            ssh: Some(0o40700),
            keys: Some(0o100600),
        };
        assert!(!fixed.too_open());
        assert_eq!(fixed.refusal().key(), "key-refused");

        // Writable by the group alone is enough for sshd to refuse.
        let group = KeyModes {
            home: Some(0o40775),
            ssh: Some(0o40700),
            keys: None,
        };
        assert!(group.too_open());
        assert_eq!(
            group.describe(),
            "home 775, .ssh 700, authorized_keys missing"
        );
        // Readable by everyone is fine.
        let readable = KeyModes {
            home: Some(0o40755),
            ssh: Some(0o40755),
            keys: Some(0o100644),
        };
        assert!(!readable.too_open());
    }

    #[test]
    fn finds_share_paths_for_both_layouts() {
        let chroot = Layout {
            home: "/home".into(),
            root: vec!["home".into(), "media".into()],
        };
        assert_eq!(chroot.share_path("media", Some("/volume1/media")), "/media");
        let admin = Layout {
            home: "/var/services/homes/anna".into(),
            root: vec!["bin".into(), "etc".into(), "volume1".into()],
        };
        assert_eq!(
            admin.share_path("media", Some("/volume1/media")),
            "/volume1/media"
        );
        assert_eq!(admin.share_path("media", None), "/media");
        assert!(chroot.chrooted());
        assert!(!admin.chrooted());
    }
}
