//! Sets up a real mount of the test NAS the way the app does (key in the app's
//! data folder, systemd user service, agent), waits until it is mounted, and
//! leaves it there for further tests. `remove` takes everything away again.
//!
//! The services start the program named in `APPIMAGE`, so point it at the app:
//!
//! ```sh
//! set -a; . ./.env.vdsm; set +a
//! APPIMAGE=$PWD/target/debug/syno-mount cargo run -p syno-mount --example systemd
//! APPIMAGE=$PWD/target/debug/syno-mount cargo run -p syno-mount --example systemd remove
//! ```

use std::time::{Duration, Instant};

use syno_dsm::Share;
use syno_mount::setup::{self, NasLink};
use syno_mount::{Address, MountStore, Paths, manager};
use syno_ssh::{KnownHosts, Target, host_alias, scan_host_key};

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set"))
}

#[tokio::main]
async fn main() {
    let host = env("VDSM_HOST");
    let ssh_port: u16 = env("VDSM_SSH_PORT").parse().unwrap();
    let account = env("VDSM_USER");
    let paths = Paths::from_env();
    let device_id = format!("{host}:{}", env("VDSM_HTTPS_PORT"));
    let alias = host_alias(&device_id);
    let target = Target {
        host: host.clone(),
        port: ssh_port,
        account: account.clone(),
        host_key: KnownHosts::new(paths.known_hosts()).get(&alias),
    };

    if std::env::args().nth(1).as_deref() == Some("remove") {
        let mut store = MountStore::load(&paths.mounts());
        let ours: Vec<_> = store
            .all()
            .iter()
            .filter(|mount| mount.device_id == device_id)
            .cloned()
            .collect();
        for mount in ours {
            store.remove(&mount.id).unwrap();
            manager::remove(&paths, &mount, &store).unwrap();
            let key = syno_ssh::KeyPair::load_or_create(&mount.key_path, "").unwrap();
            setup::remove_key(&target, &key, &account).await.unwrap();
            let _ = std::fs::remove_file(&mount.key_path);
            let _ = std::fs::remove_file(mount.key_path.with_extension("pub"));
            println!("removed {}", mount.id);
        }
        KnownHosts::new(paths.known_hosts()).forget(&alias).unwrap();
        return;
    }

    let host_key = scan_host_key(&host, ssh_port).await.unwrap();
    KnownHosts::new(paths.known_hosts())
        .pin(&alias, &host_key)
        .unwrap();
    let target = Target {
        host_key: Some(host_key.clone()),
        ..target
    };
    let key = setup::key_pair(&paths, &device_id, &account).unwrap();
    let installed = setup::install_key(&target, &env("VDSM_PASSWORD"), &key)
        .await
        .unwrap();
    let share: Share = installed
        .shares
        .iter()
        .find(|share| share.name == env("VDSM_SHARE"))
        .cloned()
        .unwrap();
    let link = NasLink {
        device_id: device_id.clone(),
        nas_name: "vdsm".into(),
        account,
        lan: Address {
            host,
            port: ssh_port,
        },
        internet: None,
        host_key_alias: alias,
        host_key_algorithm: host_key.algorithm,
        key_path: key.private_path.clone(),
        cpu: syno_dsm::CpuKind::X86,
    };
    let local = setup::default_folder(&paths, "vdsm", &share.name);
    let mut mount = setup::mount_for(&link, &installed, &share, local);
    mount.bookmark = false;
    MountStore::load(&paths.mounts())
        .upsert(mount.clone())
        .unwrap();
    manager::apply(&paths, &mount).unwrap();
    manager::start(&mount).unwrap();

    let started = Instant::now();
    loop {
        let status = manager::status(&paths, &mount);
        println!(
            "{:>4.1}s {:?}",
            started.elapsed().as_secs_f64(),
            status.state
        );
        if status.state == manager::State::Mounted {
            println!("{status:?}");
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(40),
            "not mounted: {status:?}"
        );
        std::thread::sleep(Duration::from_secs(1));
    }
    println!(
        "mounted at {}; unit {}",
        mount.local_path.display(),
        mount.unit_name()
    );
}
