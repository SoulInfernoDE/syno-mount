//! The administrator area: which DSM accounts may use SFTP and which shared folders
//! they may open. Signs in separately from the assistant and needs an administrator.

use serde::{Deserialize, Serialize};
use syno_dsm::{Access, Account, AdminShare, Client, Permission, Services, Session, ShareAccess};
use tauri::State;

use crate::devices::KnownDevice;
use crate::failure::Failure;
use crate::setup::{ConnectOutcome, ConnectRequest, sign_in};
use crate::state::AppState;

pub struct AdminSession {
    client: Client,
    session: Session,
    device: KnownDevice,
}

/// Signs in for the administrator area. Accounts outside the administrators are
/// signed out again and get `permission-denied`.
#[tauri::command]
pub async fn admin_connect(
    request: ConnectRequest,
    state: State<'_, AppState>,
) -> Result<ConnectOutcome, Failure> {
    let (client, session, device) = match sign_in(request, &state).await? {
        Ok(signed_in) => signed_in,
        Err(outcome) => return Ok(outcome),
    };
    if let Err(error) = client.services(&session).await {
        client.logout(&session).await;
        return Ok(ConnectOutcome::Failed(error.into()));
    }
    let previous = state.admin.lock().await.replace(AdminSession {
        client,
        session,
        device: device.clone(),
    });
    if let Some(previous) = previous {
        previous.client.logout(&previous.session).await;
    }
    Ok(ConnectOutcome::Connected { device })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminOverview {
    device: KnownDevice,
    services: Services,
    accounts: Vec<Account>,
    shares: Vec<AdminShare>,
}

#[tauri::command]
pub async fn admin_overview(state: State<'_, AppState>) -> Result<AdminOverview, Failure> {
    let admin = state.admin.lock().await;
    let admin = admin
        .as_ref()
        .ok_or_else(|| Failure::new("not-connected", None))?;
    Ok(AdminOverview {
        device: admin.device.clone(),
        services: admin.client.services(&admin.session).await?,
        accounts: admin.client.accounts(&admin.session).await?,
        shares: admin.client.admin_shares(&admin.session).await?,
    })
}

#[tauri::command]
pub async fn admin_share_access(
    share: String,
    state: State<'_, AppState>,
) -> Result<Vec<ShareAccess>, Failure> {
    let admin = state.admin.lock().await;
    let admin = admin
        .as_ref()
        .ok_or_else(|| Failure::new("not-connected", None))?;
    Ok(admin.client.share_access(&admin.session, &share).await?)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SftpChange {
    account: String,
    /// `None` returns to the rule for everyone.
    access: Option<Access>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareChange {
    share: String,
    account: String,
    permission: Permission,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminChanges {
    #[serde(default)]
    sftp: Vec<SftpChange>,
    #[serde(default)]
    shares: Vec<ShareChange>,
}

/// Applies the changes one after the other and stops at the first DSM refuses.
#[tauri::command]
pub async fn admin_apply(changes: AdminChanges, state: State<'_, AppState>) -> Result<(), Failure> {
    let admin = state.admin.lock().await;
    let admin = admin
        .as_ref()
        .ok_or_else(|| Failure::new("not-connected", None))?;
    for change in &changes.sftp {
        admin
            .client
            .set_sftp(&admin.session, &change.account, change.access)
            .await?;
    }
    // One call per shared folder.
    let mut shares: Vec<&str> = changes.shares.iter().map(|c| c.share.as_str()).collect();
    shares.sort_unstable();
    shares.dedup();
    for share in shares {
        let list: Vec<(String, Permission)> = changes
            .shares
            .iter()
            .filter(|change| change.share == share)
            .map(|change| (change.account.clone(), change.permission))
            .collect();
        admin
            .client
            .set_share_access(&admin.session, share, &list)
            .await?;
    }
    Ok(())
}

/// Switches on SFTP and the user home service, as the assistant does.
#[tauri::command]
pub async fn admin_enable_services(state: State<'_, AppState>) -> Result<Services, Failure> {
    let admin = state.admin.lock().await;
    let admin = admin
        .as_ref()
        .ok_or_else(|| Failure::new("not-connected", None))?;
    let current = admin.client.services(&admin.session).await?;
    let volume = admin
        .client
        .admin_shares(&admin.session)
        .await?
        .into_iter()
        .map(|share| share.volume)
        .find(|volume| volume.starts_with("/volume"))
        .unwrap_or_else(|| "/volume1".into());
    admin
        .client
        .enable_services(&admin.session, current, &volume)
        .await?;
    Ok(admin.client.services(&admin.session).await?)
}

#[tauri::command]
pub async fn admin_disconnect(state: State<'_, AppState>) -> Result<(), Failure> {
    if let Some(admin) = state.admin.lock().await.take() {
        admin.client.logout(&admin.session).await;
    }
    Ok(())
}
