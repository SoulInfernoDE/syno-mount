//! What Syno Mount needs from DSM besides signing in: the SFTP service, the user
//! home service, the shared folders an account may see, and the kind of processor.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{Client, Error, Session};

/// The DSM services an SFTP mount depends on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Services {
    /// Control Panel › File Services › FTP › SFTP.
    pub sftp: bool,
    pub sftp_port: u16,
    /// Control Panel › User & Group › Advanced › User Home. Without it there is no
    /// home folder, so no place for `~/.ssh/authorized_keys`.
    pub home: bool,
}

/// A shared folder the signed-in account can see.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Share {
    pub name: String,
    /// "/volume1/media"; `None` when DSM does not tell this account.
    pub volume_path: Option<String>,
    pub writable: bool,
}

/// Processor family of the NAS, which decides the fastest SSH cipher.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CpuKind {
    /// Intel and AMD: AES instructions, AES-GCM is fastest.
    X86,
    /// Realtek, Marvell, Annapurna and other ARM chips: ChaCha20 is faster.
    Arm,
    Unknown,
}

impl CpuKind {
    pub(crate) fn from_vendor(vendor: &str) -> Self {
        let vendor = vendor.trim().to_ascii_lowercase();
        if vendor.is_empty() {
            Self::Unknown
        } else if ["intel", "amd"].iter().any(|x86| vendor.contains(x86)) {
            Self::X86
        } else {
            Self::Arm
        }
    }
}

fn flag(value: &Value) -> bool {
    match value {
        Value::Bool(flag) => *flag,
        Value::Number(number) => number.as_i64() == Some(1),
        Value::String(text) => matches!(text.as_str(), "1" | "true" | "yes"),
        _ => false,
    }
}

fn port(value: &Value, default: u16) -> u16 {
    value
        .as_u64()
        .or_else(|| value.as_str().and_then(|text| text.parse().ok()))
        .and_then(|port| u16::try_from(port).ok())
        .filter(|port| *port > 0)
        .unwrap_or(default)
}

pub(crate) fn shares_from_api(data: &Value) -> Vec<Share> {
    let mut shares: Vec<Share> = data["shares"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|share| {
            let name = share["name"].as_str()?.to_owned();
            let additional = &share["additional"];
            let volume_path = additional["real_path"]
                .as_str()
                .filter(|path| path.starts_with('/'))
                .map(str::to_owned);
            let writable = additional["perm"]["acl"]["write"]
                .as_bool()
                .or_else(|| {
                    additional["perm"]["share_right"]
                        .as_str()
                        .map(|r| r == "RW")
                })
                .unwrap_or(true);
            Some(Share {
                name,
                volume_path,
                writable,
            })
        })
        .collect();
    shares.sort_by_key(|share| share.name.to_lowercase());
    shares
}

impl Client {
    /// Whether SFTP and the user home service are on. Needs an administrator.
    pub async fn services(&self, session: &Session) -> Result<Services, Error> {
        let sftp = self
            .call(session, "SYNO.Core.FileServ.FTP.SFTP", "get", 1, &[])
            .await?;
        let home = self
            .call(session, "SYNO.Core.User.Home", "get", 1, &[])
            .await?;
        Ok(Services {
            sftp: flag(&sftp["enable"]),
            sftp_port: port(&sftp["portnum"], 22),
            home: flag(&home["enable"]),
        })
    }

    /// Switches SFTP on (keeping its port) and the user home service on, with the
    /// homes on `home_volume` ("/volume1"). Only what is off is changed. Needs an
    /// administrator.
    pub async fn enable_services(
        &self,
        session: &Session,
        current: Services,
        home_volume: &str,
    ) -> Result<(), Error> {
        if !current.sftp {
            let port = current.sftp_port.to_string();
            self.call(
                session,
                "SYNO.Core.FileServ.FTP.SFTP",
                "set",
                1,
                &[("enable", "true"), ("portnum", &port)],
            )
            .await?;
        }
        if !current.home {
            self.call(
                session,
                "SYNO.Core.User.Home",
                "set",
                1,
                // DSM refuses the change (error 3103) without a volume.
                &[
                    ("enable", "true"),
                    ("location", home_volume),
                    ("enable_recycle_bin", "false"),
                ],
            )
            .await?;
        }
        Ok(())
    }

    /// Shared folders the account may see, through File Station, so it works for
    /// accounts outside the administrators too.
    pub async fn shares(&self, session: &Session) -> Result<Vec<Share>, Error> {
        let data = self
            .call(
                session,
                "SYNO.FileStation.List",
                "list_share",
                2,
                &[
                    ("additional", r#"["real_path","perm","volume_status"]"#),
                    ("limit", "1000"),
                ],
            )
            .await?;
        Ok(shares_from_api(&data))
    }

    /// The processor family; `Unknown` when the account may not read it.
    pub async fn cpu_kind(&self, session: &Session) -> CpuKind {
        self.call(session, "SYNO.Core.System", "info", 1, &[])
            .await
            .ok()
            .and_then(|system| system["cpu_vendor"].as_str().map(CpuKind::from_vendor))
            .unwrap_or(CpuKind::Unknown)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn reads_shares_sorted_with_paths_and_rights() {
        let data = json!({ "shares": [
            { "name": "video", "isdir": true, "additional": {
                "real_path": "/volume1/video",
                "perm": { "share_right": "RO", "acl": { "write": false } } } },
            { "name": "Backup", "isdir": true, "additional": {
                "real_path": "/volume2/Backup", "perm": { "share_right": "RW" } } },
            { "name": "home", "isdir": true, "additional": {} },
        ]});
        let shares = shares_from_api(&data);
        let names: Vec<_> = shares.iter().map(|share| share.name.as_str()).collect();
        assert_eq!(names, ["Backup", "home", "video"]);
        assert_eq!(shares[0].volume_path.as_deref(), Some("/volume2/Backup"));
        assert!(shares[0].writable);
        assert_eq!(shares[1].volume_path, None);
        assert!(!shares[2].writable);
    }

    #[test]
    fn tells_x86_from_arm() {
        assert_eq!(CpuKind::from_vendor("INTEL"), CpuKind::X86);
        assert_eq!(CpuKind::from_vendor("AMD"), CpuKind::X86);
        assert_eq!(CpuKind::from_vendor("Realtek"), CpuKind::Arm);
        assert_eq!(CpuKind::from_vendor("MARVELL"), CpuKind::Arm);
        assert_eq!(CpuKind::from_vendor(""), CpuKind::Unknown);
    }

    #[test]
    fn reads_flags_and_ports_in_any_form() {
        assert!(flag(&json!(true)) && flag(&json!(1)) && flag(&json!("yes")));
        assert!(!flag(&json!(false)) && !flag(&json!(null)));
        assert_eq!(port(&json!("2222"), 22), 2222);
        assert_eq!(port(&json!(0), 22), 22);
    }
}
