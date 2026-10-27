//! Runs the whole SSH setup against a test NAS, for example the Virtual DSM:
//!
//! ```sh
//! set -a; . ./.env.vdsm; set +a
//! cargo run -p syno-ssh --example vdsm
//! ```

use syno_ssh::{KeyPair, Login, Target, connect, scan_host_key};

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set"))
}

#[tokio::main]
async fn main() {
    let host = env("VDSM_HOST");
    let port: u16 = env("VDSM_SSH_PORT").parse().unwrap();
    let account = env("VDSM_USER");
    let password = env("VDSM_PASSWORD");

    let host_key = scan_host_key(&host, port).await.expect("host key");
    println!("host key: {} {}", host_key.algorithm, host_key.fingerprint);
    let target = Target {
        host,
        port,
        account,
        host_key: Some(host_key),
    };

    // KEEP_KEY=<path> leaves the key at <path> installed, to try sshfs by hand.
    let keep = std::env::var("KEEP_KEY").ok();
    let dir = tempfile::tempdir().unwrap();
    let path = keep
        .clone()
        .map(Into::into)
        .unwrap_or_else(|| dir.path().join("key"));
    let key = KeyPair::load_or_create(&path, "syno-mount:example").unwrap();

    let sftp = connect(&target, Login::Password(&password))
        .await
        .expect("password login");
    println!("layout with password: {:?}", sftp.layout().await.unwrap());
    sftp.install_key(&key.authorized_line())
        .await
        .expect("install key");
    drop(sftp);

    let sftp = connect(&target, Login::Key(&key.key))
        .await
        .expect("key login");
    println!(
        "key login works; home {}",
        sftp.layout().await.unwrap().home
    );
    if keep.is_some() {
        return;
    }
    sftp.remove_key("syno-mount:example")
        .await
        .expect("remove key");
    drop(sftp);

    match connect(&target, Login::Key(&key.key)).await {
        Err(error) => println!("after removal the key is refused: {error}"),
        Ok(_) => panic!("the key still works after removal"),
    }
}
