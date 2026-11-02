//! The tunnel through a reverse proxy, seen from a mount: which address it uses,
//! and the `syno-mount tunnel <id>` command that ssh runs as its `ProxyCommand`.

use crate::config::{Address, MountStore};
use crate::paths::Paths;

/// The tunnelled address of a mount; only the internet address can be one.
pub fn address(paths: &Paths, id: &str) -> Result<Address, String> {
    let store = MountStore::load(&paths.mounts());
    let mount = store
        .get(id)
        .ok_or_else(|| format!("no mount with the ID {id}"))?;
    mount
        .internet
        .clone()
        .filter(|address| address.tunnel.is_some())
        .ok_or_else(|| format!("{} has no address behind a proxy", mount.share))
}

/// Connects stdin and stdout to the SSH service of the mount's NAS, until ssh ends.
pub fn run(paths: &Paths, id: &str) -> Result<(), String> {
    let address = address(paths, id)?;
    let tunnel = address.tunnel.as_ref().ok_or("no tunnel")?;
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| error.to_string())?
        .block_on(syno_ssh::tunnel::pipe_stdio(
            &address.host,
            address.port,
            tunnel,
        ))
        .map_err(|error| error.to_string())
}
