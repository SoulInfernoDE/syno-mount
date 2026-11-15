//! Shows which address `run-mount` would take, and why the others were not taken:
//!
//! ```sh
//! cargo run -p syno-mount --example route -- 192.168.1.20:22 nas.example.org:2222
//! ```

use std::time::Duration;

use syno_mount::status;
use syno_mount::{Address, Mount};

fn address(text: &str) -> Address {
    let (host, port) = text.rsplit_once(':').expect("host:port");
    Address {
        host: host.trim_matches(['[', ']']).to_owned(),
        port: port.parse().expect("port"),
        tunnel: None,
    }
}

fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let lan = address(arguments.first().expect("LAN host:port"));
    let internet = arguments.get(1).map(|text| address(text));
    let mount = Mount {
        id: "route".into(),
        device_id: "route".into(),
        nas_name: "NAS".into(),
        share: String::new(),
        remote_path: "/".into(),
        local_path: std::env::temp_dir(),
        account: String::new(),
        lan,
        internet,
        host_key_alias: String::new(),
        host_key_algorithm: String::new(),
        key_path: std::env::temp_dir(),
        remote_uid: None,
        remote_gid: None,
        tuning: Default::default(),
        at_login: false,
        bookmark: false,
    };
    match status::choose(&mount, Duration::from_millis(1500)) {
        Ok((route, address)) => println!("{route:?}: {}", address.label()),
        Err(reason) => println!("none: {reason}"),
    }
}
