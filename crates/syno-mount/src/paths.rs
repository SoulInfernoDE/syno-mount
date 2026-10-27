//! Where Syno Mount keeps its files. The app and the commands systemd starts must
//! agree on them, so they follow the XDG base directories, not Tauri's own folders.

use std::env;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    /// `~/.config/syno-mount`: the list of NAS and mounts.
    pub config: PathBuf,
    /// `~/.local/share/syno-mount`: keys, `known_hosts`, owner maps.
    pub data: PathBuf,
    /// `~/.local/state/syno-mount`: which address each mount uses right now.
    pub state: PathBuf,
    /// `~/.config/systemd/user`.
    pub units: PathBuf,
    /// `$XDG_CONFIG_HOME` itself, for files of other programs.
    pub xdg_config: PathBuf,
    pub home: PathBuf,
}

fn base(variable: &str, home: &std::path::Path, fallback: &str) -> PathBuf {
    env::var_os(variable)
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| home.join(fallback))
}

impl Paths {
    pub fn from_env() -> Self {
        let home = env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| "/".into());
        Self::with_home(home)
    }

    pub fn with_home(home: PathBuf) -> Self {
        let config = base("XDG_CONFIG_HOME", &home, ".config");
        Self {
            data: base("XDG_DATA_HOME", &home, ".local/share").join("syno-mount"),
            state: base("XDG_STATE_HOME", &home, ".local/state").join("syno-mount"),
            units: config.join("systemd/user"),
            config: config.join("syno-mount"),
            xdg_config: config,
            home,
        }
    }

    pub fn known_hosts(&self) -> PathBuf {
        self.data.join("known_hosts")
    }

    pub fn key(&self, name: &str) -> PathBuf {
        self.data.join("keys").join(name)
    }

    pub fn owner_map(&self, mount_id: &str, kind: &str) -> PathBuf {
        self.data.join("owners").join(format!("{mount_id}.{kind}"))
    }

    pub fn mounts(&self) -> PathBuf {
        self.config.join("mounts.json")
    }

    pub fn devices(&self) -> PathBuf {
        self.config.join("devices.json")
    }

    /// GTK's bookmark list, shown by Nemo, Nautilus and the GTK file dialog.
    pub fn bookmarks(&self) -> PathBuf {
        self.xdg_config.join("gtk-3.0/bookmarks")
    }
}
