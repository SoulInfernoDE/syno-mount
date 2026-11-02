//! Reads and changes account access on a test NAS, then puts everything back:
//!
//! ```sh
//! set -a; . ./.env.vdsm; set +a
//! cargo run -p syno-dsm --example admin
//! ```

use syno_dsm::{Access, Client, Credentials, Error, Permission};

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set"))
}

#[tokio::main]
async fn main() {
    let host = env("VDSM_HOST");
    let port: u16 = env("VDSM_HTTPS_PORT").parse().unwrap();
    let credentials = Credentials {
        account: env("VDSM_ADMIN_USER"),
        password: env("VDSM_ADMIN_PASSWORD"),
        device_name: "Syno Mount example".into(),
        ..Credentials::default()
    };
    let mut client = Client::new(&host, port, None).unwrap();
    let session = match client.login(&credentials).await {
        Err(Error::UntrustedCertificate { fingerprint }) => {
            client = Client::new(&host, port, Some(&fingerprint)).unwrap();
            client.login(&credentials).await.unwrap()
        }
        other => other.unwrap(),
    };
    let user = env("VDSM_USER");
    let share = env("VDSM_SHARE");

    for account in client.accounts(&session).await.unwrap() {
        println!("{account:?}");
    }
    println!("{:?}", client.admin_shares(&session).await.unwrap());
    let before = client.share_access(&session, &share).await.unwrap();
    let own = before
        .iter()
        .find(|a| a.account == user)
        .unwrap()
        .permission;
    println!("{user} on {share}: {own:?}");

    client
        .set_share_access(&session, &share, &[(user.clone(), Permission::ReadOnly)])
        .await
        .unwrap();
    let changed = client.share_access(&session, &share).await.unwrap();
    assert_eq!(
        changed
            .iter()
            .find(|a| a.account == user)
            .unwrap()
            .permission,
        Permission::ReadOnly
    );
    client
        .set_share_access(&session, &share, &[(user.clone(), own)])
        .await
        .unwrap();

    client
        .set_sftp(&session, &user, Some(Access::Denied))
        .await
        .unwrap();
    let denied = client.accounts(&session).await.unwrap();
    assert_eq!(
        denied.iter().find(|a| a.name == user).unwrap().sftp,
        Access::Denied
    );
    client.set_sftp(&session, &user, None).await.unwrap();
    let back = client.accounts(&session).await.unwrap();
    assert_eq!(
        back.iter().find(|a| a.name == user).unwrap().sftp,
        Access::Allowed
    );
    client.logout(&session).await;
    println!("changed and restored: read-only, SFTP denied");
}
