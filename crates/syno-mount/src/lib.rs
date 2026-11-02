//! Syno Mount's core: Synology shared folders mounted over SFTP (sshfs and systemd
//! on Linux, rclone and WinFsp on Windows), kept mounted by an agent, and checked
//! for what they can do.
//!
//! The pieces do not depend on the user interface, so Syno Companion can take
//! them over as they are.

#[cfg(unix)]
pub mod bookmarks;
pub mod config;
#[cfg(unix)]
mod linux;
pub mod manager;
pub mod paths;
pub mod rclone;
pub mod selftest;
pub mod setup;
pub mod sshfs;
pub mod status;
pub mod system;
#[cfg(unix)]
pub mod systemd;
#[cfg(windows)]
mod windows;

pub use config::{Address, Cache, Mount, MountStore, Tuning};
pub use paths::Paths;
