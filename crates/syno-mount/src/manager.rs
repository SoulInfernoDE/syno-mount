//! A mount's life on this computer: files written, started, watched, and everything
//! removed again. Linux uses sshfs and systemd, Windows rclone and WinFsp; both
//! offer the same functions.

#[cfg(unix)]
pub use crate::linux::*;
pub use crate::status::{MountStatus, State};
#[cfg(windows)]
pub use crate::windows::*;
