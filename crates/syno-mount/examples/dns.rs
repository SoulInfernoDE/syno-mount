//! Shows how this computer looks up names, as the status page does:
//!
//! ```sh
//! cargo run -p syno-mount --example dns -- nas.example.org 192.168.1.20 nas.local
//! ```

fn main() {
    for host in std::env::args().skip(1) {
        println!("{host}: {:?}", syno_mount::dns::resolution(&host));
    }
}
