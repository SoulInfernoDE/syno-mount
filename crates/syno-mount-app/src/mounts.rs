//! Commands for the mounts that exist: status, start and stop, settings, tests,
//! and removal.

use std::process::Command;

use serde::{Deserialize, Serialize};
use syno_mount::manager::{self, MountStatus};
use syno_mount::selftest::{self, Check, Speed};
use syno_mount::system::{self, SystemCheck};
use syno_mount::{Address, Mount, MountStore, Tuning};
use syno_ssh::{KeyPair, KnownHosts, Target};
use tauri::State;
use tauri::async_runtime::spawn_blocking;

use crate::failure::Failure;
use crate::state::AppState;

/// Runs blocking work (systemctl, file system calls on a mount) off the UI thread.
async fn blocking<T: Send + 'static>(
    job: impl FnOnce() -> Result<T, Failure> + Send + 'static,
) -> Result<T, Failure> {
    spawn_blocking(job)
        .await
        .map_err(|error| Failure::new("unexpected", error.to_string()))?
}

fn find(state: &AppState, id: &str) -> Result<Mount, Failure> {
    let mut store = state
        .mounts
        .lock()
        .map_err(|_| Failure::new("storage", None))?;
    // The file may have changed since the app started.
    *store = MountStore::load(&state.paths.mounts());
    store
        .get(id)
        .cloned()
        .ok_or_else(|| Failure::new("not-found", id.to_owned()))
}

#[tauri::command]
pub async fn system_check() -> Result<SystemCheck, Failure> {
    blocking(|| Ok(system::check())).await
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MountView {
    mount: Mount,
    status: MountStatus,
}

#[tauri::command]
pub async fn list_mounts(state: State<'_, AppState>) -> Result<Vec<MountView>, Failure> {
    let mounts = {
        let mut store = state
            .mounts
            .lock()
            .map_err(|_| Failure::new("storage", None))?;
        *store = MountStore::load(&state.paths.mounts());
        store.all().to_vec()
    };
    let paths = state.paths.clone();
    blocking(move || {
        // One thread per mount: a hanging one must not hold up the others.
        let handles: Vec<_> = mounts
            .into_iter()
            .map(|mount| {
                let paths = paths.clone();
                std::thread::spawn(move || {
                    let status = manager::status(&paths, &mount);
                    MountView { mount, status }
                })
            })
            .collect();
        Ok(handles
            .into_iter()
            .filter_map(|handle| handle.join().ok())
            .collect())
    })
    .await
}

#[tauri::command]
pub async fn start_mount(id: String, state: State<'_, AppState>) -> Result<(), Failure> {
    let mount = find(&state, &id)?;
    let paths = state.paths.clone();
    blocking(move || {
        // Units may be missing after a move to another program path; write them again.
        manager::apply(&paths, &mount)?;
        Ok(manager::start(&mount)?)
    })
    .await
}

#[tauri::command]
pub async fn stop_mount(id: String, state: State<'_, AppState>) -> Result<(), Failure> {
    let mount = find(&state, &id)?;
    blocking(move || Ok(manager::stop(&mount)?)).await
}

/// The settings a user may change after setup.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MountSettings {
    at_login: bool,
    bookmark: bool,
    internet: Option<Address>,
    tuning: Tuning,
}

#[tauri::command]
pub async fn update_mount(
    id: String,
    settings: MountSettings,
    state: State<'_, AppState>,
) -> Result<Mount, Failure> {
    let mut mount = find(&state, &id)?;
    mount.at_login = settings.at_login;
    mount.bookmark = settings.bookmark;
    mount.internet = settings
        .internet
        .filter(|address| !address.host.trim().is_empty());
    mount.tuning = settings.tuning;
    state
        .mounts
        .lock()
        .map_err(|_| Failure::new("storage", None))?
        .upsert(mount.clone())?;
    let paths = state.paths.clone();
    let applied = mount.clone();
    blocking(move || {
        let running = syno_mount::status::is_mounted(&applied.local_path);
        manager::apply(&paths, &applied)?;
        if running {
            // New options take effect with a new connection.
            syno_mount::systemd::restart(&applied.unit_name())?;
        }
        Ok(())
    })
    .await?;
    Ok(mount)
}

/// Removes a mount from this computer. With `remove_key`, and when no other mount
/// of this NAS and account is left, the key is removed from the NAS as well, and
/// the host key and key file are forgotten.
#[tauri::command]
pub async fn remove_mount(
    id: String,
    remove_key: bool,
    state: State<'_, AppState>,
) -> Result<(), Failure> {
    let mount = find(&state, &id)?;
    let others_use_key = {
        let mut store = state
            .mounts
            .lock()
            .map_err(|_| Failure::new("storage", None))?;
        store.remove(&id)?;
        store
            .all()
            .iter()
            .any(|other| other.key_path == mount.key_path)
    };
    let paths = state.paths.clone();
    let removed = mount.clone();
    blocking(move || {
        let store = MountStore::load(&paths.mounts());
        Ok(manager::remove(&paths, &removed, &store)?)
    })
    .await?;

    if remove_key && !others_use_key {
        let known = KnownHosts::new(state.paths.known_hosts());
        let target = Target {
            host: mount.lan.host.clone(),
            port: mount.lan.port,
            account: mount.account.clone(),
            host_key: known.get(&mount.host_key_alias),
        };
        if let Ok(key) = KeyPair::load_or_create(&mount.key_path, "") {
            syno_mount::setup::remove_key(&target, &key, &mount.account).await?;
        }
        let _ = std::fs::remove_file(&mount.key_path);
        let _ = std::fs::remove_file(mount.key_path.with_extension("pub"));
        let still_used = state
            .mounts
            .lock()
            .map(|store| {
                store
                    .all()
                    .iter()
                    .any(|other| other.host_key_alias == mount.host_key_alias)
            })
            .unwrap_or(true);
        if !still_used {
            known.forget(&mount.host_key_alias)?;
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn self_test(id: String, state: State<'_, AppState>) -> Result<Vec<Check>, Failure> {
    let mount = find(&state, &id)?;
    blocking(move || {
        selftest::run(&mount.local_path).map_err(|error| Failure::new("selftest", error))
    })
    .await
}

#[tauri::command]
pub async fn speed_test(
    id: String,
    megabytes: Option<u32>,
    state: State<'_, AppState>,
) -> Result<Speed, Failure> {
    let mount = find(&state, &id)?;
    let megabytes = megabytes.unwrap_or(128).clamp(8, 2048);
    blocking(move || {
        selftest::speed(&mount.local_path, megabytes)
            .map_err(|error| Failure::new("selftest", error))
    })
    .await
}

#[tauri::command]
pub async fn open_folder(id: String, state: State<'_, AppState>) -> Result<(), Failure> {
    let mount = find(&state, &id)?;
    Command::new("xdg-open")
        .arg(&mount.local_path)
        .spawn()
        .map(|_| ())
        .map_err(Failure::io)
}

/// The sshfs command the service runs, for the user to read or copy.
#[tauri::command]
pub async fn command_line(id: String, state: State<'_, AppState>) -> Result<String, Failure> {
    let mount = find(&state, &id)?;
    let paths = state.paths.clone();
    blocking(move || Ok(manager::command_line(&paths, &mount))).await
}
