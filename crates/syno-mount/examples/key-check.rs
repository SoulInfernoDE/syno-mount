//! Installs the key as the assistant does and reports whether the NAS takes it,
//! and if not, why (for example a home folder with Windows ACLs). Removes the key
//! again when it works.
//!
//! ```sh
//! set -a; . ./.env.vdsm; set +a
//! cargo run -p syno-mount --example key-check
//! ```

use syno_mount::paths::Paths;
use syno_mount::setup;
use syno_ssh::{Target, scan_host_key};

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set"))
}

#[tokio::main]
async fn main() {
    let host = env("VDSM_HOST");
    let port: u16 = env("VDSM_SSH_PORT").parse().unwrap();
    let account = env("VDSM_USER");
    let password = env("VDSM_PASSWORD");

    let root = tempfile::tempdir().unwrap();
    let paths = Paths {
        data: root.path().join("data"),
        ..Paths::with_home(root.path().to_owned())
    };
    let host_key = scan_host_key(&host, port, None).await.expect("host key");
    let target = Target {
        host: host.clone(),
        port,
        account: account.clone(),
        host_key: Some(host_key),
        tunnel: None,
    };
    let key = setup::key_pair(&paths, &host, &account).unwrap();
    match setup::install_key(&target, &password, &key).await {
        Ok(installed) => {
            println!("key login works; home {}", installed.layout.home);
            setup::remove_key(&target, &key).await.expect("remove key");
        }
        Err(error) => println!("refused: {} ({error})", error.key()),
    }
}
