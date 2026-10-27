//! The steps between signing in to DSM and a mount: the key, its installation on
//! the NAS, and the mount entries for the chosen shared folders.

use std::fs;
use std::path::PathBuf;

use syno_dsm::{CpuKind, Share};
use syno_ssh::{KeyPair, Layout, Login, Target};

use crate::config::{Address, Mount, Tuning, slug};
use crate::paths::Paths;

/// This computer's name, so the user can tell keys apart in `authorized_keys`.
fn hostname() -> String {
    fs::read_to_string("/proc/sys/kernel/hostname")
        .map(|name| name.trim().to_owned())
        .ok()
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "linux".into())
}

/// "syno-mount:anna@laptop": the comment that marks the app's key on the NAS.
pub fn key_comment(account: &str) -> String {
    format!("{}{account}@{}", syno_ssh::KEY_MARKER, hostname())
}

/// The key for one NAS and account, created on first use.
pub fn key_pair(paths: &Paths, device_id: &str, account: &str) -> Result<KeyPair, syno_ssh::Error> {
    let name = format!("{}-{}", slug(device_id), slug(account));
    KeyPair::load_or_create(&paths.key(&name), &key_comment(account))
}

/// What the NAS told while the key was installed.
#[derive(Debug, Clone)]
pub struct Installed {
    pub layout: Layout,
    pub uid: Option<u32>,
    pub gid: Option<u32>,
    /// Shared folders as SFTP shows them; the choice when DSM's Web API is not
    /// open to the account.
    pub shares: Vec<Share>,
}

/// Installs the key with the account's password, then signs in with the key to
/// prove that sshd takes it.
pub async fn install_key(
    target: &Target,
    password: &str,
    key: &KeyPair,
) -> Result<Installed, syno_ssh::Error> {
    {
        let sftp = syno_ssh::connect(target, Login::Password(password)).await?;
        sftp.install_key(&key.authorized_line()).await?;
    }
    inspect(target, key).await
}

/// Signs in with the key and reads layout, owner and shared folders.
pub async fn inspect(target: &Target, key: &KeyPair) -> Result<Installed, syno_ssh::Error> {
    let sftp = syno_ssh::connect(target, Login::Key(&key.key)).await?;
    let layout = sftp.layout().await?;
    let (uid, gid) = sftp.owner().await?;
    let shares = sftp
        .shares()
        .await?
        .into_iter()
        .map(|(name, path)| Share {
            name,
            volume_path: Some(path),
            writable: true,
        })
        .collect();
    Ok(Installed {
        layout,
        uid,
        gid,
        shares,
    })
}

/// Removes the app's key from the NAS, signing in with that key.
pub async fn remove_key(
    target: &Target,
    key: &KeyPair,
    account: &str,
) -> Result<(), syno_ssh::Error> {
    let sftp = syno_ssh::connect(target, Login::Key(&key.key)).await?;
    sftp.remove_key(&key_comment(account)).await
}

/// Everything about a NAS that all its mounts share.
#[derive(Debug, Clone)]
pub struct NasLink {
    pub device_id: String,
    pub nas_name: String,
    pub account: String,
    pub lan: Address,
    pub internet: Option<Address>,
    pub host_key_alias: String,
    pub host_key_algorithm: String,
    pub key_path: PathBuf,
    pub cpu: CpuKind,
}

/// `~/NAS/<NAS name>/<share>`.
pub fn default_folder(paths: &Paths, nas_name: &str, share: &str) -> PathBuf {
    let clean = |text: &str| text.replace(['/', ','], "-");
    paths
        .home
        .join("NAS")
        .join(clean(nas_name))
        .join(clean(share))
}

pub fn mount_for(
    link: &NasLink,
    installed: &Installed,
    share: &Share,
    local_path: PathBuf,
) -> Mount {
    Mount {
        id: Mount::make_id(&link.device_id, &share.name),
        device_id: link.device_id.clone(),
        nas_name: link.nas_name.clone(),
        share: share.name.clone(),
        remote_path: installed
            .layout
            .share_path(&share.name, share.volume_path.as_deref()),
        local_path,
        account: link.account.clone(),
        lan: link.lan.clone(),
        internet: link.internet.clone(),
        host_key_alias: link.host_key_alias.clone(),
        host_key_algorithm: link.host_key_algorithm.clone(),
        key_path: link.key_path.clone(),
        remote_uid: installed.uid,
        remote_gid: installed.gid,
        tuning: Tuning {
            cpu: link.cpu,
            ..Tuning::default()
        },
        at_login: true,
        bookmark: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folders_and_comments_are_safe() {
        let paths = Paths::with_home("/home/anna".into());
        assert_eq!(
            default_folder(&paths, "nas/home", "a,b"),
            PathBuf::from("/home/anna/NAS/nas-home/a-b")
        );
        assert!(key_comment("anna").starts_with("syno-mount:anna@"));
    }
}
