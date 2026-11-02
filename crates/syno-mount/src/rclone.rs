//! The `rclone mount` command line for a mount on Windows. rclone talks SFTP itself
//! and mounts through WinFsp; no rclone config file is written, the SFTP settings
//! travel in an on-the-fly remote (`:sftp,host=…:/path`).

use std::path::Path;

use syno_dsm::CpuKind;

use crate::config::{Address, Cache, Mount};
use crate::paths::Paths;

/// A value in an rclone connection string. Values with a comma, colon, quote or
/// space are wrapped in double quotes, inner quotes doubled.
fn quote(value: &str) -> String {
    if value
        .chars()
        .any(|c| matches!(c, ',' | ':' | '"' | '\'' | ' ' | '='))
    {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}

/// Ciphers in the order rclone should offer them, space separated.
pub fn ciphers(cpu: CpuKind) -> &'static str {
    match cpu {
        CpuKind::Arm => {
            "chacha20-poly1305@openssh.com aes128-gcm@openssh.com aes256-gcm@openssh.com aes128-ctr"
        }
        CpuKind::X86 | CpuKind::Unknown => {
            "aes128-gcm@openssh.com aes256-gcm@openssh.com chacha20-poly1305@openssh.com aes128-ctr"
        }
    }
}

/// The name of an address in `known_hosts`, as ssh and rclone look it up:
/// "nas.local" for port 22, "[nas.local]:2222" otherwise.
pub fn known_hosts_name(address: &Address) -> String {
    if address.port == 22 {
        address.host.clone()
    } else {
        format!("[{}]:{}", address.host, address.port)
    }
}

/// The SFTP remote: `:sftp,host=…,user=…,key_file=…:/remote/path`.
pub fn remote(mount: &Mount, paths: &Paths, address: &Address) -> String {
    let settings = [
        ("host", address.host.clone()),
        ("port", address.port.to_string()),
        ("user", mount.account.clone()),
        ("key_file", mount.key_path.to_string_lossy().into_owned()),
        (
            "known_hosts_file",
            paths.known_hosts().to_string_lossy().into_owned(),
        ),
        // Only the pinned key type: DSM offers several, and rclone reports any other
        // one as a key mismatch.
        ("host_key_algorithms", mount.host_key_algorithm.clone()),
        ("ciphers", ciphers(mount.tuning.cpu).to_owned()),
        // DSM's SFTP has no shell, so rclone must not try to run md5sum and the like.
        ("disable_hashcheck", "true".to_owned()),
        ("shell_type", "none".to_owned()),
        ("set_modtime", "true".to_owned()),
    ];
    let joined: Vec<String> = settings
        .iter()
        .map(|(key, value)| format!("{key}={}", quote(value)))
        .collect();
    format!(":sftp,{}:{}", joined.join(","), mount.remote_path)
}

/// Arguments after `rclone` that mount `mount` over `address` until the process ends.
pub fn arguments(mount: &Mount, paths: &Paths, address: &Address) -> Vec<String> {
    let target = mount.local_path.to_string_lossy();
    // "Z:\" and "Z:" both mean the drive; rclone wants "Z:".
    let target = target.trim_end_matches(['\\', '/']).to_owned();
    let mut arguments = vec![
        "mount".to_owned(),
        remote(mount, paths, address),
        target,
        // Windows treats network drives as slow and unreliable, which suits SFTP.
        "--network-mode".into(),
        "--volname".into(),
        format!("{} ({})", mount.share, mount.nas_name),
        // Programs may write anywhere in a file; rclone then needs a local copy.
        "--vfs-cache-mode".into(),
        "writes".into(),
        "--transfers".into(),
        mount.tuning.connections.max(1).to_string(),
        "--contimeout".into(),
        "10s".into(),
        "--timeout".into(),
        "60s".into(),
    ];
    match mount.tuning.cache {
        Cache::Normal => arguments.extend(["--dir-cache-time".into(), "20s".into()]),
        Cache::Fresh => arguments.extend(["--dir-cache-time".into(), "0s".into()]),
    }
    arguments
}

/// The command as one line for the user to read, with Windows quoting.
pub fn display(program: &Path, arguments: &[String]) -> String {
    std::iter::once(program.to_string_lossy().into_owned())
        .chain(arguments.iter().cloned())
        .map(|argument| {
            if argument.is_empty() || argument.contains([' ', '"', ',']) {
                format!("\"{}\"", argument.replace('"', "\\\""))
            } else {
                argument
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::tests::sample;

    #[test]
    fn quotes_values_rclone_would_split() {
        assert_eq!(quote("nas.local"), "nas.local");
        assert_eq!(quote(r"C:\Users\anna\key"), r#""C:\Users\anna\key""#);
        assert_eq!(quote(r#"a"b"#), r#""a""b""#);
        assert_eq!(
            quote("aes128-gcm@openssh.com aes128-ctr"),
            r#""aes128-gcm@openssh.com aes128-ctr""#
        );
    }

    #[test]
    fn names_addresses_like_known_hosts() {
        let lan = Address {
            host: "192.168.1.20".into(),
            port: 22,
        };
        let internet = Address {
            host: "nas.example.org".into(),
            port: 2222,
        };
        assert_eq!(known_hosts_name(&lan), "192.168.1.20");
        assert_eq!(known_hosts_name(&internet), "[nas.example.org]:2222");
    }

    #[test]
    fn builds_a_network_drive_with_host_key_check() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::with_home(dir.path().to_owned());
        let mut mount = sample(dir.path());
        mount.local_path = "Z:\\".into();
        let arguments = arguments(&mount, &paths, &mount.lan);
        assert_eq!(arguments[0], "mount");
        assert!(arguments[1].starts_with(":sftp,host=192.168.1.20,port=22,user=anna,key_file="));
        assert!(arguments[1].contains(",known_hosts_file="));
        assert!(arguments[1].contains(",host_key_algorithms=ssh-ed25519,"));
        assert!(arguments[1].contains(",shell_type=none,"));
        assert!(arguments[1].ends_with(":/Media"));
        assert_eq!(arguments[2], "Z:");
        for flag in ["--network-mode", "--vfs-cache-mode", "--volname"] {
            assert!(arguments.iter().any(|a| a == flag), "{flag} missing");
        }
        let line = display(Path::new(r"C:\Tools\rclone.exe"), &arguments);
        assert!(line.starts_with(r"C:\Tools\rclone.exe mount "));
        assert!(line.contains(r#""Media (nas)""#));
    }
}
