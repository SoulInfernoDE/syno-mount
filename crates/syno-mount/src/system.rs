//! Can this computer mount over SSHFS at all?

use std::fs::OpenOptions;
use std::path::Path;
use std::process::Command;

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemCheck {
    /// "3.7.3"; `None` when sshfs is not installed.
    pub sshfs: Option<String>,
    /// Parallel connections need 3.7 or newer.
    pub sshfs_parallel: bool,
    pub fusermount: bool,
    /// `/dev/fuse` can be opened by this user.
    pub fuse_device: bool,
    /// A systemd user instance runs, so mounts can start at login.
    pub systemd_user: bool,
    /// The command that installs what is missing, for the distribution in use.
    pub install_hint: Option<String>,
}

impl SystemCheck {
    pub fn ready(&self) -> bool {
        self.sshfs.is_some() && self.fusermount && self.fuse_device
    }
}

fn found(program: &str) -> bool {
    Command::new(program)
        .arg("--version")
        .output()
        .map(|output| output.status.success() || !output.stdout.is_empty())
        .unwrap_or(false)
}

/// The package manager command for `/etc/os-release`'s `ID` and `ID_LIKE`.
pub fn install_command(os_release: &str) -> Option<String> {
    let field = |key: &str| {
        os_release
            .lines()
            .find_map(|line| line.strip_prefix(&format!("{key}=")))
            .map(|value| value.trim_matches('"').to_lowercase())
            .unwrap_or_default()
    };
    let ids = format!("{} {}", field("ID"), field("ID_LIKE"));
    let has = |name: &str| ids.split_whitespace().any(|id| id == name);
    if has("debian") || has("ubuntu") {
        Some("sudo apt install sshfs".into())
    } else if has("fedora") || has("rhel") {
        Some("sudo dnf install fuse-sshfs".into())
    } else if has("arch") {
        Some("sudo pacman -S sshfs".into())
    } else if has("suse") || has("opensuse") {
        Some("sudo zypper install sshfs".into())
    } else {
        None
    }
}

pub fn check() -> SystemCheck {
    let version = crate::sshfs::installed_version();
    let fuse_device = OpenOptions::new()
        .read(true)
        .write(true)
        .open(Path::new("/dev/fuse"))
        .is_ok();
    let os_release = std::fs::read_to_string("/etc/os-release").unwrap_or_default();
    let mut check = SystemCheck {
        sshfs: version.map(|(major, minor, patch)| format!("{major}.{minor}.{patch}")),
        sshfs_parallel: version.is_some_and(|version| version >= (3, 7, 0)),
        fusermount: found("fusermount3"),
        fuse_device,
        systemd_user: crate::systemd::available(),
        install_hint: None,
    };
    if !check.ready() {
        check.install_hint = install_command(&os_release);
    }
    check
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suggests_the_right_package_manager() {
        let mint = "NAME=\"Linux Mint\"\nID=linuxmint\nID_LIKE=\"ubuntu debian\"\n";
        assert_eq!(
            install_command(mint).as_deref(),
            Some("sudo apt install sshfs")
        );
        let fedora = "ID=fedora\n";
        assert_eq!(
            install_command(fedora).as_deref(),
            Some("sudo dnf install fuse-sshfs")
        );
        let manjaro = "ID=manjaro\nID_LIKE=arch\n";
        assert_eq!(
            install_command(manjaro).as_deref(),
            Some("sudo pacman -S sshfs")
        );
        assert_eq!(install_command("ID=gentoo\n"), None);
    }
}
