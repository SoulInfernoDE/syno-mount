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

/// Removes the lines of the key with `comment` from `text`.
pub(crate) fn without_key(text: &str, comment: &str) -> String {
    let kept: Vec<&str> = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter(|line| line.split_whitespace().nth(2) != Some(comment))
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

    /// Removes the key with `comment` from `~/.ssh/authorized_keys`.
    pub async fn remove_key(&self, comment: &str) -> Result<(), Error> {
        let home = self.home().await?;
        let file = format!("{}/.ssh/authorized_keys", home.trim_end_matches('/'));
        if !self.session.try_exists(&file).await? {
            return Ok(());
        }
        let current = String::from_utf8_lossy(&self.session.read(&file).await?).into_owned();
        let next = without_key(&current, comment);
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
        assert_eq!(
            without_key(&text, "syno-mount:anna@laptop"),
            format!("{THEIRS}\n")
        );
        assert_eq!(
            without_key(&format!("{OURS}\n"), "syno-mount:anna@laptop"),
            ""
        );
        assert_eq!(without_key(&text, "syno-mount:other"), text);
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
