//! Lists the Synology NAS on the local network: `cargo run -p syno-discovery --example browse`

fn main() {
    match syno_discovery::browse(std::time::Duration::from_secs(4)) {
        Ok(found) if found.is_empty() => println!("No Synology NAS answered."),
        Ok(found) => found.iter().for_each(|nas| println!("{nas:?}")),
        Err(error) => eprintln!("Search failed: {error}"),
    }
}
