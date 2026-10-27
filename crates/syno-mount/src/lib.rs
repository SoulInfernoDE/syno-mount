//! Syno Mount's core: Synology shared folders mounted over SSHFS, kept mounted by
//! systemd user services and an agent, and checked for what they can do.
//!
//! The pieces do not depend on the user interface, so Syno Companion can take
//! them over as they are.

pub mod bookmarks;
pub mod config;
pub mod manager;
pub mod paths;
pub mod selftest;
pub mod setup;
pub mod sshfs;
pub mod status;
pub mod system;
pub mod systemd;

pub use config::{Address, Cache, Mount, MountStore, Tuning};
pub use paths::Paths;
