//! The assistant: sign in to DSM (or go by SSH alone), check the services, trust
//! the host key, install the key, and create the mounts for the chosen folders.
//! The password lives only in memory while the assistant is open.

use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use syno_discovery::FoundNas;
use syno_dsm::{AuthError, Client, CpuKind, Credentials, Error, Services, Session, Share};
use syno_mount::setup::{self as core, Installed, NasLink};
use syno_mount::{Address, Mount, manager};
use syno_ssh::{HostKey, KnownHosts, Target, host_alias};
use tauri::State;
use tauri::async_runtime::spawn_blocking;

use crate::devices::{KnownDevice, device_id};
use crate::failure::Failure;
use crate::state::AppState;

const DISCOVERY_TIME: Duration = Duration::from_secs(3);

/// What the assistant has learned so far.
pub struct Pending {
    pub device: KnownDevice,
    dsm: Option<(Client, Session)>,
    password: Option<String>,
    /// Where SSH answers on the LAN.
    ssh: Address,
    scanned: Option<HostKey>,
    trusted: Option<HostKey>,
    installed: Option<Installed>,
    dsm_shares: Option<Vec<Share>>,
    cpu: CpuKind,
    /// The internet address of a NAS that has mounts already.
    internet: Option<Address>,
}

impl Pending {
    fn target(&self) -> Target {
        Target {
            host: self.ssh.host.clone(),
            port: self.ssh.port,
            account: self.device.account.clone(),
            host_key: self.trusted.clone(),
        }
    }
}

fn device_name() -> String {
    "Syno Mount (Linux)".to_owned()
}

#[tauri::command]
pub async fn discover() -> Result<Vec<FoundNas>, Failure> {
    spawn_blocking(|| syno_discovery::browse(DISCOVERY_TIME))
        .await
        .map_err(|error| Failure::new("unexpected", error.to_string()))?
        .map_err(|error| Failure::new("discovery", error))
}

#[tauri::command]
pub fn known_devices(state: State<'_, AppState>) -> Vec<KnownDevice> {
    state
        .devices
        .lock()
        .map(|store| store.all().to_vec())
        .unwrap_or_default()
}

#[tauri::command]
pub fn forget_device(id: String, state: State<'_, AppState>) -> Result<(), Failure> {
    state
        .devices
        .lock()
        .map_err(|_| Failure::new("storage", None))?
        .remove(&id)
        .map(|_| ())
        .map_err(Failure::io)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectRequest {
    host: String,
    port: u16,
    name: Option<String>,
    model: Option<String>,
    account: String,
    password: String,
    otp: Option<String>,
    trust_fingerprint: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum ConnectOutcome {
    Connected {
        device: KnownDevice,
    },
    OtpRequired,
    UntrustedCertificate {
        fingerprint: String,
    },
    CertificateChanged {
        fingerprint: String,
    },
    /// DSM refuses this account (no DSM permission); SSH may still work.
    NotAllowed,
    Failed(Failure),
}

/// Signs in to DSM and starts the assistant for this NAS.
#[tauri::command]
pub async fn connect(
    request: ConnectRequest,
    state: State<'_, AppState>,
) -> Result<ConnectOutcome, Failure> {
    let host = request.host.trim().to_owned();
    let id = device_id(&host, request.port);
    let account = request.account.trim().to_owned();
    let known = state
        .devices
        .lock()
        .ok()
        .and_then(|store| store.get(&id).cloned());
    let pin = request
        .trust_fingerprint
        .clone()
        .or_else(|| known.as_ref().and_then(|device| device.fingerprint.clone()));

    let client = Client::new(&host, request.port, pin.as_deref())?;
    let credentials = Credentials {
        account: account.clone(),
        password: request.password.clone(),
        otp: request.otp.clone().filter(|code| !code.trim().is_empty()),
        device_name: device_name(),
        ..Credentials::default()
    };
    let session = match client.login(&credentials).await {
        Ok(session) => session,
        Err(Error::UntrustedCertificate { fingerprint }) => {
            return Ok(ConnectOutcome::UntrustedCertificate { fingerprint });
        }
        Err(Error::CertificateChanged { fingerprint }) => {
            return Ok(ConnectOutcome::CertificateChanged { fingerprint });
        }
        Err(Error::Auth(AuthError::OtpRequired)) => return Ok(ConnectOutcome::OtpRequired),
        Err(Error::Auth(AuthError::PermissionDenied)) => return Ok(ConnectOutcome::NotAllowed),
        Err(error) => return Ok(ConnectOutcome::Failed(error.into())),
    };

    let (name, model) = client.identity(&session).await;
    let device = KnownDevice {
        id: id.clone(),
        name: name
            .ok()
            .or(request.name)
            .or_else(|| known.as_ref().map(|device| device.name.clone()))
            .unwrap_or_else(|| host.clone()),
        model: model
            .ok()
            .or(request.model)
            .or_else(|| known.as_ref().map(|device| device.model.clone()))
            .unwrap_or_default(),
        host: host.clone(),
        port: request.port,
        account,
        fingerprint: pin,
    };
    if let Ok(mut store) = state.devices.lock() {
        store.upsert(device.clone()).map_err(Failure::io)?;
    }
    let dsm_shares = client.shares(&session).await.ok();
    let cpu = client.cpu_kind(&session).await;
    let ssh_port = client
        .services(&session)
        .await
        .map(|services| services.sftp_port)
        .unwrap_or(22);
    let previous = state.pending.lock().await.replace(Pending {
        device: device.clone(),
        dsm: Some((client, session)),
        password: Some(request.password),
        ssh: Address {
            host,
            port: ssh_port,
        },
        scanned: None,
        trusted: None,
        installed: None,
        dsm_shares,
        cpu,
        internet: None,
    });
    sign_out(previous).await;
    Ok(ConnectOutcome::Connected { device })
}

async fn sign_out(pending: Option<Pending>) {
    if let Some((client, session)) = pending.and_then(|pending| pending.dsm) {
        client.logout(&session).await;
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SshOnlyRequest {
    host: String,
    /// Port of DSM's web interface: identifies the NAS in the list.
    dsm_port: u16,
    ssh_port: u16,
    name: Option<String>,
    account: String,
    password: String,
}

/// Starts the assistant without DSM's Web API, for accounts DSM does not let in.
#[tauri::command]
pub async fn connect_ssh_only(
    request: SshOnlyRequest,
    state: State<'_, AppState>,
) -> Result<KnownDevice, Failure> {
    let host = request.host.trim().to_owned();
    let id = device_id(&host, request.dsm_port);
    let known = state
        .devices
        .lock()
        .ok()
        .and_then(|store| store.get(&id).cloned());
    let device = KnownDevice {
        id,
        name: request
            .name
            .filter(|name| !name.trim().is_empty())
            .or_else(|| known.as_ref().map(|device| device.name.clone()))
            .unwrap_or_else(|| host.clone()),
        model: known.as_ref().map(|d| d.model.clone()).unwrap_or_default(),
        host: host.clone(),
        port: request.dsm_port,
        account: request.account.trim().to_owned(),
        fingerprint: known.and_then(|device| device.fingerprint),
    };
    let previous = state.pending.lock().await.replace(Pending {
        device: device.clone(),
        dsm: None,
        password: Some(request.password),
        ssh: Address {
            host,
            port: request.ssh_port,
        },
        scanned: None,
        trusted: None,
        installed: None,
        dsm_shares: None,
        cpu: CpuKind::Unknown,
        internet: None,
    });
    sign_out(previous).await;
    Ok(device)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServicesView {
    /// `None` when the account may not read them (not an administrator) or there
    /// is no DSM session.
    services: Option<Services>,
    ssh_port: u16,
}

#[tauri::command]
pub async fn setup_services(state: State<'_, AppState>) -> Result<ServicesView, Failure> {
    let pending = state.pending.lock().await;
    let pending = pending
        .as_ref()
        .ok_or_else(|| Failure::new("not-connected", None))?;
    let services = match &pending.dsm {
        Some((client, session)) => client.services(session).await.ok(),
        None => None,
    };
    Ok(ServicesView {
        services,
        ssh_port: pending.ssh.port,
    })
}

/// Switches on SFTP and the user home service. Needs an administrator.
#[tauri::command]
pub async fn enable_services(state: State<'_, AppState>) -> Result<ServicesView, Failure> {
    let mut guard = state.pending.lock().await;
    let pending = guard
        .as_mut()
        .ok_or_else(|| Failure::new("not-connected", None))?;
    let (client, session) = pending
        .dsm
        .as_ref()
        .ok_or_else(|| Failure::new("not-connected", None))?;
    let current = client.services(session).await?;
    // Homes go on the volume of the first shared folder DSM reports.
    let volume = pending
        .dsm_shares
        .iter()
        .flatten()
        .find_map(|share| {
            let path = share.volume_path.as_deref()?;
            let volume = path.split('/').nth(1)?;
            volume.starts_with("volume").then(|| format!("/{volume}"))
        })
        .unwrap_or_else(|| "/volume1".into());
    client.enable_services(session, current, &volume).await?;
    let services = client.services(session).await?;
    pending.ssh.port = services.sftp_port;
    Ok(ServicesView {
        services: Some(services),
        ssh_port: services.sftp_port,
    })
}

/// Reads the host key of the NAS for the user to compare and trust. A key pinned
/// earlier for this NAS is reported as `pinned`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostKeyView {
    key: HostKey,
    /// The same key was trusted before.
    known: bool,
    /// A different key was trusted before.
    changed: bool,
}

#[tauri::command]
pub async fn scan_host_key(
    port: Option<u16>,
    state: State<'_, AppState>,
) -> Result<HostKeyView, Failure> {
    let mut guard = state.pending.lock().await;
    let pending = guard
        .as_mut()
        .ok_or_else(|| Failure::new("not-connected", None))?;
    if let Some(port) = port {
        pending.ssh.port = port;
    }
    let key = syno_ssh::scan_host_key(&pending.ssh.host, pending.ssh.port).await?;
    let pinned = KnownHosts::new(state.paths.known_hosts()).get(&host_alias(&pending.device.id));
    let known = pinned.as_ref() == Some(&key);
    let changed = pinned.is_some() && !known;
    pending.scanned = Some(key.clone());
    Ok(HostKeyView {
        key,
        known,
        changed,
    })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareChoice {
    name: String,
    /// The folder on the NAS as SFTP sees it.
    remote_path: String,
    default_folder: PathBuf,
    /// Already mounted on this computer.
    mounted: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Installation {
    shares: Vec<ShareChoice>,
    /// DSM keeps the account in a chroot: only its shared folders are visible.
    chrooted: bool,
}

fn choices(state: &AppState, pending: &Pending, installed: &Installed) -> Vec<ShareChoice> {
    let mounts = state
        .mounts
        .lock()
        .map(|store| store.all().to_vec())
        .unwrap_or_default();
    // DSM's list knows shares the account cannot open over SFTP; only offer what SFTP shows.
    let shares: Vec<Share> = match &pending.dsm_shares {
        Some(dsm) => installed
            .shares
            .iter()
            .map(|sftp| {
                dsm.iter()
                    .find(|share| share.name == sftp.name)
                    .cloned()
                    .unwrap_or_else(|| sftp.clone())
            })
            .collect(),
        None => installed.shares.clone(),
    };
    shares
        .into_iter()
        .map(|share| ShareChoice {
            remote_path: installed
                .layout
                .share_path(&share.name, share.volume_path.as_deref()),
            default_folder: core::default_folder(&state.paths, &pending.device.name, &share.name),
            mounted: mounts
                .iter()
                .any(|mount| mount.id == Mount::make_id(&pending.device.id, &share.name)),
            name: share.name,
        })
        .collect()
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallRequest {
    /// The fingerprint the user compared and trusted.
    trust_fingerprint: String,
    /// For SSH, when it differs from the DSM password or there was no DSM sign-in.
    password: Option<String>,
}

/// Pins the host key, creates the key and installs it on the NAS.
#[tauri::command]
pub async fn install_key(
    request: InstallRequest,
    state: State<'_, AppState>,
) -> Result<Installation, Failure> {
    let mut guard = state.pending.lock().await;
    let pending = guard
        .as_mut()
        .ok_or_else(|| Failure::new("not-connected", None))?;
    let scanned = pending
        .scanned
        .clone()
        .filter(|key| key.fingerprint == request.trust_fingerprint)
        .ok_or_else(|| Failure::new("host-key-unknown", None))?;
    if let Some(password) = request.password.filter(|p| !p.is_empty()) {
        pending.password = Some(password);
    }
    let password = pending
        .password
        .clone()
        .ok_or_else(|| Failure::new("ssh-auth-failed", None))?;
    pending.trusted = Some(scanned.clone());

    let key = core::key_pair(&state.paths, &pending.device.id, &pending.device.account)?;
    let installed = core::install_key(&pending.target(), &password, &key).await?;
    KnownHosts::new(state.paths.known_hosts()).pin(&host_alias(&pending.device.id), &scanned)?;
    let chrooted = installed.layout.chrooted();
    let shares = choices(&state, pending, &installed);
    pending.installed = Some(installed);
    // The key works now; the password is not needed any more.
    pending.password = None;
    Ok(Installation { shares, chrooted })
}

/// More shared folders from a NAS that has a mount already: signs in with the key.
#[tauri::command]
pub async fn more_shares(
    mount_id: String,
    state: State<'_, AppState>,
) -> Result<Installation, Failure> {
    let mount = state
        .mounts
        .lock()
        .ok()
        .and_then(|store| store.get(&mount_id).cloned())
        .ok_or_else(|| Failure::new("not-found", None))?;
    let host_key = KnownHosts::new(state.paths.known_hosts()).get(&mount.host_key_alias);
    let target = Target {
        host: mount.lan.host.clone(),
        port: mount.lan.port,
        account: mount.account.clone(),
        host_key: host_key.clone(),
    };
    let key =
        syno_ssh::KeyPair::load_or_create(&mount.key_path, &core::key_comment(&mount.account))?;
    let installed = core::inspect(&target, &key).await?;
    let known = state
        .devices
        .lock()
        .ok()
        .and_then(|store| store.get(&mount.device_id).cloned());
    let device = known.unwrap_or_else(|| KnownDevice {
        id: mount.device_id.clone(),
        name: mount.nas_name.clone(),
        model: String::new(),
        host: mount.lan.host.clone(),
        port: 5001,
        account: mount.account.clone(),
        fingerprint: None,
    });
    let pending = Pending {
        device,
        dsm: None,
        password: None,
        ssh: mount.lan.clone(),
        scanned: host_key.clone(),
        trusted: host_key,
        installed: None,
        dsm_shares: None,
        cpu: mount.tuning.cpu,
        internet: mount.internet.clone(),
    };
    let chrooted = installed.layout.chrooted();
    let shares = choices(&state, &pending, &installed);
    let pending = Pending {
        installed: Some(installed),
        ..pending
    };
    let previous = state.pending.lock().await.replace(pending);
    sign_out(previous).await;
    Ok(Installation { shares, chrooted })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChosenShare {
    name: String,
    local_path: PathBuf,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateRequest {
    shares: Vec<ChosenShare>,
    at_login: bool,
    bookmark: bool,
    internet: Option<Address>,
}

/// Creates, writes and starts the mounts, then ends the assistant.
#[tauri::command]
pub async fn create_mounts(
    request: CreateRequest,
    state: State<'_, AppState>,
) -> Result<Vec<Mount>, Failure> {
    let pending = state
        .pending
        .lock()
        .await
        .take()
        .ok_or_else(|| Failure::new("not-connected", None))?;
    let installed = pending
        .installed
        .clone()
        .ok_or_else(|| Failure::new("not-connected", None))?;
    let host_key = pending
        .trusted
        .clone()
        .ok_or_else(|| Failure::new("host-key-unknown", None))?;
    let key = core::key_pair(&state.paths, &pending.device.id, &pending.device.account)?;
    let internet = request
        .internet
        .filter(|address| !address.host.trim().is_empty())
        .or_else(|| pending.internet.clone());
    let link = NasLink {
        device_id: pending.device.id.clone(),
        nas_name: pending.device.name.clone(),
        account: pending.device.account.clone(),
        lan: pending.ssh.clone(),
        internet,
        host_key_alias: host_alias(&pending.device.id),
        host_key_algorithm: host_key.algorithm.clone(),
        key_path: key.private_path.clone(),
        cpu: pending.cpu,
    };
    let mut created = Vec::new();
    for chosen in request.shares {
        let share = installed
            .shares
            .iter()
            .find(|share| share.name == chosen.name)
            .cloned()
            .ok_or_else(|| Failure::new("not-found", chosen.name.clone()))?;
        let mut mount = core::mount_for(&link, &installed, &share, chosen.local_path);
        mount.at_login = request.at_login;
        mount.bookmark = request.bookmark;
        created.push(mount);
    }
    sign_out(Some(pending)).await;

    let paths = state.paths.clone();
    let to_apply = created.clone();
    {
        let mut store = state
            .mounts
            .lock()
            .map_err(|_| Failure::new("storage", None))?;
        for mount in &to_apply {
            store.upsert(mount.clone())?;
        }
    }
    spawn_blocking(move || -> Result<(), Failure> {
        for mount in &to_apply {
            manager::apply(&paths, mount)?;
            manager::start(mount)?;
        }
        Ok(())
    })
    .await
    .map_err(|error| Failure::new("unexpected", error.to_string()))??;
    Ok(created)
}

/// Leaves the assistant; signs out of DSM and forgets the password.
#[tauri::command]
pub async fn cancel_setup(state: State<'_, AppState>) -> Result<(), Failure> {
    let previous = state.pending.lock().await.take();
    sign_out(previous).await;
    Ok(())
}
