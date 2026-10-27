//! The `sshfs` command line for a mount, and what the installed sshfs can do.

use std::path::Path;
use std::process::Command;

use syno_dsm::CpuKind;

use crate::config::{Address, Cache, Mount};
use crate::paths::Paths;

/// Which address a mount uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Route {
    Lan,
    Internet,
}

/// "3.7.3" → (3, 7, 3).
pub fn parse_version(text: &str) -> Option<(u32, u32, u32)> {
    let line = text
        .lines()
        .find(|line| line.to_lowercase().contains("sshfs version"))?;
    let version = line.split_whitespace().last()?;
    let mut parts = version.split('.').map(|part| part.parse::<u32>().ok());
    Some((
        parts.next()??,
        parts.next().flatten().unwrap_or(0),
        parts.next().flatten().unwrap_or(0),
    ))
}

/// Version of the `sshfs` on this computer, or `None` when it is missing.
pub fn installed_version() -> Option<(u32, u32, u32)> {
    let output = Command::new("sshfs").arg("--version").output().ok()?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    parse_version(&text)
}

/// Ciphers in the order ssh should try them; the NAS picks the first it knows.
pub fn ciphers(cpu: CpuKind) -> &'static str {
    match cpu {
        CpuKind::Arm => {
            "chacha20-poly1305@openssh.com,aes128-gcm@openssh.com,aes256-gcm@openssh.com,aes128-ctr"
        }
        CpuKind::X86 | CpuKind::Unknown => {
            "aes128-gcm@openssh.com,aes256-gcm@openssh.com,chacha20-poly1305@openssh.com,aes128-ctr"
        }
    }
}

/// sshfs splits `-o` values at commas, so no path may contain one.
fn checked(path: &Path) -> Result<String, String> {
    let text = path.to_string_lossy().into_owned();
    if text.contains(',') {
        return Err(format!(
            "a path with a comma cannot be passed to sshfs: {text}"
        ));
    }
    Ok(text)
}

/// Arguments after `sshfs` that mount `mount` over `address` in the foreground.
pub fn arguments(
    mount: &Mount,
    paths: &Paths,
    address: &Address,
    route: Route,
    version: (u32, u32, u32),
) -> Result<Vec<String>, String> {
    let tuning = &mount.tuning;
    let mut options = vec![
        // ssh: only this key, only the pinned host key, never a prompt.
        format!("Port={}", address.port),
        format!("IdentityFile={}", checked(&mount.key_path)?),
        "IdentitiesOnly=yes".into(),
        format!("UserKnownHostsFile={}", checked(&paths.known_hosts())?),
        "GlobalKnownHostsFile=/dev/null".into(),
        format!("HostKeyAlias={}", mount.host_key_alias),
        format!("HostKeyAlgorithms={}", mount.host_key_algorithm),
        "StrictHostKeyChecking=yes".into(),
        "BatchMode=yes".into(),
        "PasswordAuthentication=no".into(),
        "ConnectTimeout=10".into(),
        "ServerAliveInterval=15".into(),
        "ServerAliveCountMax=3".into(),
        // FUSE splits options at commas; a backslash keeps them in the value.
        format!("Ciphers={}", ciphers(tuning.cpu).replace(',', "\\,")),
        format!(
            "Compression={}",
            if route == Route::Internet && tuning.compress_internet {
                "yes"
            } else {
                "no"
            }
        ),
        // sshfs and FUSE.
        "reconnect".into(),
        format!("fsname=syno-mount:{}", mount.id),
        "auto_unmount".into(),
    ];
    if version >= (3, 7, 0) && tuning.connections > 1 {
        options.push(format!("max_conns={}", tuning.connections));
    }
    match tuning.cache {
        Cache::Normal => {
            options.push("dir_cache=yes".into());
            options.push("dcache_timeout=20".into());
        }
        Cache::Fresh => options.push("dir_cache=no".into()),
    }
    if mount.remote_uid.is_some() || mount.remote_gid.is_some() {
        // Own files on the NAS show as one's own here; others keep their numbers.
        options.push("idmap=file".into());
        options.push(format!(
            "uidfile={}",
            checked(&paths.owner_map(&mount.id, "uid"))?
        ));
        options.push(format!(
            "gidfile={}",
            checked(&paths.owner_map(&mount.id, "gid"))?
        ));
        options.push("nomap=ignore".into());
    } else {
        options.push("idmap=user".into());
    }

    let mut arguments = vec!["-f".to_owned()];
    for option in options {
        arguments.push("-o".into());
        arguments.push(option);
    }
    arguments.push(format!(
        "{}@{}:{}",
        mount.account, address.host, mount.remote_path
    ));
    arguments.push(checked(&mount.local_path)?);
    Ok(arguments)
}

/// The command as one line for the user to read, with simple quoting.
pub fn display(arguments: &[String]) -> String {
    std::iter::once("sshfs".to_owned())
        .chain(arguments.iter().map(|argument| {
            if argument
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "-_=./:@,+".contains(c))
            {
                argument.clone()
            } else {
                format!("'{}'", argument.replace('\'', r"'\''"))
            }
        }))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::tests::sample;

    #[test]
    fn reads_the_version() {
        assert_eq!(
            parse_version("SSHFS version 3.7.3\nFUSE library version 3.14.0"),
            Some((3, 7, 3))
        );
        assert_eq!(parse_version("SSHFS version 2.10"), Some((2, 10, 0)));
        assert_eq!(parse_version("nothing"), None);
    }

    #[test]
    fn builds_a_strict_foreground_command() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::with_home(dir.path().to_owned());
        let mount = sample(dir.path());
        let arguments = arguments(&mount, &paths, &mount.lan, Route::Lan, (3, 7, 3)).unwrap();
        let line = display(&arguments);
        assert_eq!(arguments[0], "-f");
        for expected in [
            "StrictHostKeyChecking=yes",
            "BatchMode=yes",
            "HostKeyAlias=syno-mount.nas.local_5001",
            "Compression=no",
            "max_conns=4",
            "idmap=file",
            "fsname=syno-mount:nas-local-5001-media",
            r"Ciphers=aes128-gcm@openssh.com\,",
        ] {
            assert!(line.contains(expected), "{expected} missing in {line}");
        }
        assert!(line.ends_with(&format!(
            "anna@192.168.1.20:/Media {}",
            mount.local_path.display()
        )));
    }

    #[test]
    fn adapts_to_the_route_the_cpu_and_old_sshfs() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::with_home(dir.path().to_owned());
        let mut mount = sample(dir.path());
        mount.tuning.cpu = CpuKind::Arm;
        mount.remote_uid = None;
        mount.remote_gid = None;
        let internet = mount.internet.clone().unwrap();
        let line =
            display(&arguments(&mount, &paths, &internet, Route::Internet, (3, 6, 0)).unwrap());
        assert!(line.contains("Port=2222"));
        assert!(line.contains("Compression=yes"));
        assert!(line.contains(r"Ciphers=chacha20-poly1305@openssh.com\,"));
        assert!(line.contains("idmap=user"));
        assert!(!line.contains("max_conns"));
        assert!(line.contains("anna@nas.example.org:/Media"));
    }

    #[test]
    fn refuses_paths_with_commas() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::with_home(dir.path().to_owned());
        let mut mount = sample(dir.path());
        mount.local_path = dir.path().join("a,b");
        assert!(arguments(&mount, &paths, &mount.lan, Route::Lan, (3, 7, 3)).is_err());
    }

    #[test]
    fn quotes_what_needs_quoting() {
        let line = display(&["-o".into(), "a b".into(), "it's".into()]);
        assert_eq!(line, r"sshfs -o 'a b' 'it'\''s'");
    }
}
