//! The whole chain against a test NAS such as the Virtual DSM, without systemd and
//! without touching the user's own files: sign in to DSM, list shares, pin the host
//! key, install the key, mount with the app's sshfs command, run the self-test and
//! the speed test, unmount, remove the key.
//!
//! ```sh
//! set -a; . ./.env.vdsm; set +a
//! cargo run -p syno-mount --example vdsm
//! ```

use std::process::Command;
use std::time::{Duration, Instant};

use syno_dsm::{Client, CpuKind, Credentials, Error, Share};
use syno_mount::paths::Paths;
use syno_mount::setup::{self, NasLink};
use syno_mount::sshfs::{self, Route};
use syno_mount::{Address, selftest, status};
use syno_ssh::{KnownHosts, Target, host_alias, scan_host_key};

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set"))
}

/// Shares and processor from DSM, or `None` when the account may not sign in.
async fn dsm_shares(
    host: &str,
    port: u16,
    account: &str,
    password: &str,
) -> Option<(Vec<Share>, CpuKind)> {
    let credentials = Credentials {
        account: account.to_owned(),
        password: password.to_owned(),
        device_name: "Syno Mount example".into(),
        ..Credentials::default()
    };
    let mut client = Client::new(host, port, None).unwrap();
    let session = match client.login(&credentials).await {
        Err(Error::UntrustedCertificate { fingerprint }) => {
            client = Client::new(host, port, Some(&fingerprint)).unwrap();
            client.login(&credentials).await
        }
        other => other,
    };
    let session = match session {
        Ok(session) => session,
        Err(error) => {
            println!("DSM: {error}; using SFTP only");
            return None;
        }
    };
    match client.services(&session).await {
        Ok(services) => println!("services: {services:?}"),
        Err(error) => println!("services: {error} (expected outside the administrators)"),
    }
    let shares = client.shares(&session).await.ok();
    let cpu = client.cpu_kind(&session).await;
    client.logout(&session).await;
    shares.map(|shares| (shares, cpu))
}

#[tokio::main]
async fn main() {
    let host = env("VDSM_HOST");
    let https: u16 = env("VDSM_HTTPS_PORT").parse().unwrap();
    let ssh_port: u16 = env("VDSM_SSH_PORT").parse().unwrap();
    let account = env("VDSM_USER");
    let password = env("VDSM_PASSWORD");
    let share_name = env("VDSM_SHARE");

    // DSM is optional: accounts without DSM access still get their shares over SFTP.
    let from_dsm = dsm_shares(&host, https, &account, &password).await;
    // Everything below lives in a temporary folder.
    let root = tempfile::tempdir().unwrap();
    let paths = Paths::with_home(root.path().to_owned());
    let paths = Paths {
        config: root.path().join("config"),
        data: root.path().join("data"),
        state: root.path().join("state"),
        ..paths
    };
    let device_id = format!("{host}:{https}");
    let alias = host_alias(&device_id);
    let host_key = scan_host_key(&host, ssh_port).await.expect("host key");
    KnownHosts::new(paths.known_hosts())
        .pin(&alias, &host_key)
        .unwrap();
    let target = Target {
        host: host.clone(),
        port: ssh_port,
        account: account.clone(),
        host_key: Some(host_key.clone()),
    };
    let key = setup::key_pair(&paths, &device_id, &account).unwrap();
    let installed = setup::install_key(&target, &password, &key)
        .await
        .expect("install key");
    println!(
        "layout {:?}, owner {:?}:{:?}",
        installed.layout, installed.uid, installed.gid
    );
    let (shares, cpu) = from_dsm.unwrap_or((installed.shares.clone(), CpuKind::Unknown));
    println!(
        "shares: {:?}",
        shares
            .iter()
            .map(|s| (&s.name, &s.volume_path))
            .collect::<Vec<_>>()
    );
    let share = shares
        .iter()
        .find(|s| s.name == share_name)
        .expect("test share")
        .clone();

    let link = NasLink {
        device_id: device_id.clone(),
        nas_name: "vdsm".into(),
        account: account.clone(),
        lan: Address {
            host: host.clone(),
            port: ssh_port,
        },
        internet: None,
        host_key_alias: alias,
        host_key_algorithm: host_key.algorithm.clone(),
        key_path: key.private_path.clone(),
        cpu,
    };
    let local = root.path().join("mnt");
    let mount = setup::mount_for(&link, &installed, &share, local.clone());
    std::fs::create_dir_all(&local).unwrap();
    syno_mount::manager::write_owner_maps(&paths, &mount).unwrap();

    let version = sshfs::installed_version().expect("sshfs");
    let arguments = sshfs::arguments(&mount, &paths, &mount.lan, Route::Lan, version).unwrap();
    println!("{}", sshfs::display(&arguments));
    let mut child = Command::new("sshfs").args(&arguments).spawn().unwrap();
    let started = Instant::now();
    while !status::is_mounted(&local) {
        assert!(started.elapsed() < Duration::from_secs(15), "not mounted");
        std::thread::sleep(Duration::from_millis(200));
    }

    // The ACL share itself, then a folder without ACL if there is one ("posix").
    for folder in [local.clone(), local.join("posix")] {
        if !folder.is_dir() {
            continue;
        }
        println!("self-test in {}:", folder.display());
        match selftest::run(&folder) {
            Ok(checks) => {
                for check in checks {
                    println!(
                        "  {:<12} {:?} {}",
                        check.id,
                        check.outcome,
                        check.detail.unwrap_or_default()
                    );
                }
            }
            Err(error) => println!("  failed: {error}"),
        }
    }
    let speed = selftest::speed(&local, 64).unwrap();
    println!(
        "speed: write {:.0} MB/s, read {:.0} MB/s, {:.0} small files/s",
        speed.write_bytes_per_second / 1e6,
        speed.read_bytes_per_second / 1e6,
        speed.small_files_per_second
    );

    Command::new("fusermount3")
        .arg("-u")
        .arg(&local)
        .status()
        .unwrap();
    let _ = child.wait();
    setup::remove_key(&target, &key, &account)
        .await
        .expect("remove key");
    println!("done; key removed from the NAS");
}
