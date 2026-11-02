//! Can this computer mount at all? Linux needs sshfs and FUSE, Windows rclone and
//! WinFsp.

#[cfg(unix)]
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemCheck {
    /// "linux" or "windows": which of the fields below apply.
    pub platform: &'static str,
    /// "3.7.3"; `None` when sshfs is not installed.
    pub sshfs: Option<String>,
    /// Parallel connections need 3.7 or newer.
    pub sshfs_parallel: bool,
    pub fusermount: bool,
    /// `/dev/fuse` can be opened by this user.
    pub fuse_device: bool,
    /// A systemd user instance runs, so mounts can start at login.
    pub systemd_user: bool,
    /// Windows: "v1.71.0"; `None` when rclone is not found.
    pub rclone: Option<String>,
    /// Windows: WinFsp is installed.
    pub winfsp: bool,
    /// The command that installs what is missing, for the distribution in use.
    pub install_hint: Option<String>,
}

impl SystemCheck {
    pub fn ready(&self) -> bool {
        if self.platform == "windows" {
            self.rclone.is_some() && self.winfsp
        } else {
            self.sshfs.is_some() && self.fusermount && self.fuse_device
        }
    }
}

/// Where rclone is: on the PATH, or where winget and the installer put it.
pub fn rclone_path() -> Option<PathBuf> {
    let name = if cfg!(windows) {
        "rclone.exe"
    } else {
        "rclone"
    };
    let mut candidates: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|path| {
            std::env::split_paths(&path)
                .map(|dir| dir.join(name))
                .collect()
        })
        .unwrap_or_default();
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        candidates.push(Path::new(&local).join(r"Microsoft\WinGet\Links").join(name));
    }
    if let Some(programs) = std::env::var_os("ProgramFiles") {
        candidates.push(Path::new(&programs).join("rclone").join(name));
    }
    candidates.into_iter().find(|path| path.is_file())
}

/// "rclone v1.71.0" → "v1.71.0".
pub fn parse_rclone_version(text: &str) -> Option<String> {
    let line = text.lines().next()?;
    let version = line.strip_prefix("rclone ")?.trim();
    (!version.is_empty()).then(|| version.to_owned())
}

#[cfg(windows)]
fn winfsp_installed() -> bool {
    ["ProgramFiles(x86)", "ProgramFiles"]
        .iter()
        .any(|variable| {
            std::env::var_os(variable)
                .is_some_and(|dir| Path::new(&dir).join(r"WinFsp\bin\winfsp-x64.dll").is_file())
        })
}

#[cfg(unix)]
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

/// The winget command that installs what is missing on Windows.
pub const WINDOWS_INSTALL: &str =
    "winget install -e --id WinFsp.WinFsp; winget install -e --id Rclone.Rclone";

#[cfg(windows)]
pub fn check() -> SystemCheck {
    use std::os::windows::process::CommandExt;
    let rclone = rclone_path().and_then(|path| {
        let output = Command::new(path)
            .arg("version")
            .creation_flags(0x0800_0000)
            .output()
            .ok()?;
        parse_rclone_version(&String::from_utf8_lossy(&output.stdout))
    });
    let mut check = SystemCheck {
        platform: "windows",
        sshfs: None,
        sshfs_parallel: false,
        fusermount: false,
        fuse_device: false,
        systemd_user: false,
        rclone,
        winfsp: winfsp_installed(),
        install_hint: None,
    };
    if !check.ready() {
        check.install_hint = Some(WINDOWS_INSTALL.into());
    }
    check
}

#[cfg(unix)]
pub fn check() -> SystemCheck {
    let version = crate::sshfs::installed_version();
    let fuse_device = OpenOptions::new()
        .read(true)
        .write(true)
        .open(Path::new("/dev/fuse"))
        .is_ok();
    let os_release = std::fs::read_to_string("/etc/os-release").unwrap_or_default();
    let mut check = SystemCheck {
        platform: "linux",
        sshfs: version.map(|(major, minor, patch)| format!("{major}.{minor}.{patch}")),
        sshfs_parallel: version.is_some_and(|version| version >= (3, 7, 0)),
        fusermount: found("fusermount3"),
        fuse_device,
        systemd_user: crate::systemd::available(),
        rclone: None,
        winfsp: false,
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

    #[test]
    fn reads_the_rclone_version() {
        let text = "rclone v1.71.0\n- os/version: Microsoft Windows 11\n";
        assert_eq!(parse_rclone_version(text).as_deref(), Some("v1.71.0"));
        assert_eq!(parse_rclone_version("nothing"), None);
    }
}
