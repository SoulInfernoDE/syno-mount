//! NAS the user signed in to before, so the next setup can start from the list and
//! trusts the same DSM certificate. Nothing secret is stored here: Syno Mount keeps
//! no passwords; after setup the SSH key does the work.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnownDevice {
    /// `host:port`, lower case.
    pub id: String,
    pub name: String,
    pub model: String,
    pub host: String,
    pub port: u16,
    pub account: String,
    /// Fingerprint of a self-signed certificate the user trusted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
}

pub fn device_id(host: &str, port: u16) -> String {
    format!("{}:{port}", host.trim().to_lowercase())
}

/// The list of known NAS, kept as JSON in the app's config folder.
#[derive(Debug)]
pub struct DeviceStore {
    path: PathBuf,
    devices: Vec<KnownDevice>,
}

impl DeviceStore {
    /// Loads `devices.json` from `dir`; a missing or unreadable file starts an empty list.
    pub fn load(dir: &Path) -> Self {
        let path = dir.join("devices.json");
        let devices = fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Self { path, devices }
    }

    pub fn all(&self) -> &[KnownDevice] {
        &self.devices
    }

    pub fn get(&self, id: &str) -> Option<&KnownDevice> {
        self.devices.iter().find(|device| device.id == id)
    }

    /// Adds or replaces a device and moves it to the top of the list.
    pub fn upsert(&mut self, device: KnownDevice) -> io::Result<()> {
        self.devices.retain(|known| known.id != device.id);
        self.devices.insert(0, device);
        self.save()
    }

    pub fn remove(&mut self, id: &str) -> io::Result<Option<KnownDevice>> {
        let index = self.devices.iter().position(|device| device.id == id);
        let removed = index.map(|index| self.devices.remove(index));
        self.save()?;
        Ok(removed)
    }

    /// Writes to a temporary file first, so a crash never leaves half a file.
    fn save(&self) -> io::Result<()> {
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        let temporary = self.path.with_extension("json.tmp");
        fs::write(&temporary, serde_json::to_vec_pretty(&self.devices)?)?;
        fs::rename(&temporary, &self.path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(host: &str) -> KnownDevice {
        KnownDevice {
            id: device_id(host, 5001),
            name: host.into(),
            model: "DS923+".into(),
            host: host.into(),
            port: 5001,
            account: "admin".into(),
            fingerprint: None,
        }
    }

    #[test]
    fn keeps_devices_across_loads_newest_first() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = DeviceStore::load(dir.path());
        store.upsert(device("a")).unwrap();
        store.upsert(device("b")).unwrap();
        store.upsert(device("a")).unwrap();

        let reloaded = DeviceStore::load(dir.path());
        let ids: Vec<_> = reloaded.all().iter().map(|d| d.id.as_str()).collect();
        assert_eq!(ids, ["a:5001", "b:5001"]);

        let mut reloaded = reloaded;
        assert!(reloaded.remove("a:5001").unwrap().is_some());
        assert_eq!(DeviceStore::load(dir.path()).all().len(), 1);
    }

    #[test]
    fn ids_ignore_case_and_spaces() {
        assert_eq!(device_id(" NAS.local ", 5001), "nas.local:5001");
    }
}
