//! What the commands share.

use std::sync::Mutex;

use syno_mount::{MountStore, Paths};
use tauri::async_runtime::Mutex as AsyncMutex;

use crate::devices::DeviceStore;
use crate::setup::Pending;

pub struct AppState {
    pub paths: Paths,
    pub devices: Mutex<DeviceStore>,
    pub mounts: Mutex<MountStore>,
    /// The assistant, while it is open.
    pub pending: AsyncMutex<Option<Pending>>,
}

impl AppState {
    pub fn new(paths: Paths) -> Self {
        Self {
            devices: Mutex::new(DeviceStore::load(&paths.config)),
            mounts: Mutex::new(MountStore::load(&paths.mounts())),
            pending: AsyncMutex::new(None),
            paths,
        }
    }
}
