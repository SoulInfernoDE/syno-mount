//! Checks an SSH service behind a reverse proxy the way Syno Mount does: the probe
//! (TLS and the SSH banner) and the host key read through the tunnel.
//!
//! ```sh
//! cargo run -p syno-ssh --example tunnel-probe -- <proxy host> <port> <server name> [no-alpn] [pin=SHA256:…]
//! ```

use std::time::Duration;

use syno_ssh::{Tunnel, scan_host_key, tunnel};

#[tokio::main]
async fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let [host, port, server_name, options @ ..] = arguments.as_slice() else {
        eprintln!("usage: tunnel-probe <host> <port> <server name> [no-alpn] [pin=SHA256:…]");
        std::process::exit(2);
    };
    let port: u16 = port.parse().expect("port");
    let mut settings = Tunnel::new(server_name);
    for option in options {
        if option == "no-alpn" {
            settings.alpn = false;
        } else if let Some(pin) = option.strip_prefix("pin=") {
            settings.pinned_certificate = Some(pin.to_owned());
        }
    }
    let probe = {
        let host = host.clone();
        let settings = settings.clone();
        tokio::task::spawn_blocking(move || {
            tunnel::probe(&host, port, &settings, Duration::from_secs(3))
        })
        .await
        .unwrap()
    };
    match probe {
        Ok(latency) => println!("probe: ok, {:.1} ms", latency.as_secs_f64() * 1000.0),
        Err(error) => println!("probe: {} ({error})", error.key()),
    }
    match scan_host_key(host, port, Some(&settings)).await {
        Ok(key) => println!("host key: {} {}", key.algorithm, key.fingerprint),
        Err(error) => println!("host key: {} ({error})", error.key()),
    }
}
