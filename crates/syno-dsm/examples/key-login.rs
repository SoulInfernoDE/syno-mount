//! Prepares an account's home folder for key login on a test NAS, as the
//! administrator area does: prints the commands, runs them once as root through
//! the Task Scheduler and prints the outcome.
//!
//! ```sh
//! set -a; . ./.env.vdsm; set +a
//! cargo run -p syno-dsm --example key-login
//! ```

use syno_dsm::{Client, Credentials, Error, key_login_script};

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
    println!("{}", key_login_script(&user).unwrap());
    // CONFIRM_PASSWORD=wrong tries a wrong confirmation.
    let confirm =
        std::env::var("CONFIRM_PASSWORD").unwrap_or_else(|_| credentials.password.clone());
    let outcome = client
        .prepare_key_login(&session, &credentials.account, &confirm, &user)
        .await;
    println!("{outcome:?}");
    client.logout(&session).await;
}
