//! How this computer looks up the name of a NAS, and whether that DNS query is
//! encrypted. Only the way to the resolver is judged: an encrypted query still
//! shows the name to the resolver itself.

use std::collections::HashMap;
use std::net::IpAddr;
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::Serialize;

/// The answer changes with the network, but not every few seconds.
const KEEP: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Protocol {
    Tls,
    Https,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Resolution {
    /// An IP address, or a name from the hosts file: no DNS query at all.
    NoQuery,
    /// A `.local` name, answered by multicast DNS inside the LAN.
    Multicast,
    /// Encrypted on the way to the resolver.
    Encrypted {
        protocol: Protocol,
    },
    /// Encrypted when the resolver offers it, plain otherwise.
    Opportunistic,
    /// A resolver on this computer (dnscrypt-proxy, stubby, …) whose way out
    /// cannot be seen from here.
    LocalResolver,
    /// Plain DNS: everyone on the way can read the name.
    Plain,
    Unknown,
}

/// How `host` is looked up now; kept for a minute.
pub fn resolution(host: &str) -> Resolution {
    static CACHE: OnceLock<Mutex<HashMap<String, (Instant, Resolution)>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Ok(cache) = cache.lock()
        && let Some((when, known)) = cache.get(host)
        && when.elapsed() < KEEP
    {
        return known.clone();
    }
    let found = look(host);
    if let Ok(mut cache) = cache.lock() {
        cache.insert(host.to_owned(), (Instant::now(), found.clone()));
    }
    found
}

fn look(host: &str) -> Resolution {
    let host = host.trim_start_matches('[').trim_end_matches(']');
    if host.parse::<IpAddr>().is_ok() || in_hosts_file(host) {
        return Resolution::NoQuery;
    }
    if host.to_ascii_lowercase().ends_with(".local") {
        return Resolution::Multicast;
    }
    system(host)
}

#[cfg(unix)]
const HOSTS: &str = "/etc/hosts";
#[cfg(windows)]
const HOSTS: &str = r"C:\Windows\System32\drivers\etc\hosts";

fn in_hosts_file(host: &str) -> bool {
    std::fs::read_to_string(HOSTS).is_ok_and(|text| hosts_file_has(&text, host))
}

fn hosts_file_has(text: &str, host: &str) -> bool {
    text.lines()
        .map(|line| line.split('#').next().unwrap_or_default())
        .any(|line| {
            line.split_whitespace()
                .skip(1)
                .any(|name| name.eq_ignore_ascii_case(host))
        })
}

fn run(program: &str, arguments: &[&str]) -> Option<String> {
    let mut command = Command::new(program);
    command.args(arguments);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let output = command.output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

/// The name servers in `resolv.conf`.
#[cfg(unix)]
fn name_servers(text: &str) -> Vec<IpAddr> {
    text.lines()
        .filter_map(|line| line.trim().strip_prefix("nameserver"))
        .filter_map(|rest| rest.trim().split('%').next()?.parse().ok())
        .collect()
}

/// systemd-resolved's stub listens on these.
#[cfg(unix)]
fn is_resolved_stub(server: &IpAddr) -> bool {
    matches!(server.to_string().as_str(), "127.0.0.53" | "127.0.0.54")
}

/// "github.com: 140.82.121.3   -- link: wlp3s0" → "wlp3s0".
#[cfg(unix)]
fn answering_link(query: &str) -> Option<String> {
    query
        .lines()
        .find_map(|line| line.split("-- link:").nth(1))
        .map(|link| link.trim().to_owned())
        .filter(|link| !link.is_empty())
}

/// "Link 3 (wlp3s0): opportunistic" or "Global: no" → the mode.
#[cfg(unix)]
fn dot_mode(text: &str) -> Option<Resolution> {
    let mode = text.lines().find(|line| line.contains(':'))?;
    Some(match mode.rsplit(':').next()?.trim() {
        "yes" => Resolution::Encrypted {
            protocol: Protocol::Tls,
        },
        "opportunistic" => Resolution::Opportunistic,
        "no" => Resolution::Plain,
        _ => Resolution::Unknown,
    })
}

#[cfg(unix)]
fn system(host: &str) -> Resolution {
    let Ok(resolv) = std::fs::read_to_string("/etc/resolv.conf") else {
        return Resolution::Unknown;
    };
    let servers = name_servers(&resolv);
    if servers.is_empty() {
        return Resolution::Unknown;
    }
    if servers.iter().all(is_resolved_stub) {
        // systemd-resolved: the link that answered has its own DNS over TLS mode.
        let link = run("resolvectl", &["query", "--legend=no", host])
            .and_then(|query| answering_link(&query));
        let mode = match &link {
            Some(link) => run("resolvectl", &["dnsovertls", link]),
            None => run("resolvectl", &["dnsovertls"]),
        };
        return mode
            .and_then(|text| dot_mode(&text))
            .unwrap_or(Resolution::Unknown);
    }
    if servers.iter().all(IpAddr::is_loopback) {
        return Resolution::LocalResolver;
    }
    Resolution::Plain
}

/// What PowerShell finds out about the DNS servers of the interface that leads to
/// the internet, and Windows' DNS over HTTPS settings for them.
#[cfg(windows)]
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct WindowsServer {
    address: String,
    /// A DNS over HTTPS template is known for this server.
    template: bool,
    /// The interface is set to use it (`DohFlags` in the registry).
    flags: u32,
    /// Falling back to plain DNS is allowed.
    fallback: bool,
    /// Windows upgrades known servers on its own (`EnableAutoDoh`).
    auto: bool,
}

#[cfg(windows)]
const WINDOWS_SCRIPT: &str = r#"
$ErrorActionPreference = 'SilentlyContinue'
$route = Get-NetRoute -DestinationPrefix '0.0.0.0/0' | Sort-Object { $_.RouteMetric + $_.InterfaceMetric } | Select-Object -First 1
$adapter = Get-NetAdapter -InterfaceIndex $route.InterfaceIndex
$servers = (Get-DnsClientServerAddress -InterfaceIndex $route.InterfaceIndex).ServerAddresses
$doh = @{}; Get-DnsClientDohServerAddress | ForEach-Object { $doh[$_.ServerAddress] = $_ }
$auto = (Get-ItemProperty 'HKLM:\SYSTEM\CurrentControlSet\Services\Dnscache\Parameters').EnableAutoDoh -eq 2
$base = "HKLM:\SYSTEM\CurrentControlSet\Services\Dnscache\InterfaceSpecificParameters\$($adapter.InterfaceGuid)\DohInterfaceSettings"
$list = @($servers | ForEach-Object {
  $family = if ($_ -like '*:*') { 'Doh6' } else { 'Doh' }
  $flags = (Get-ItemProperty "$base\$family\$_").DohFlags
  $known = $doh[$_]
  [pscustomobject]@{
    Address = $_; Template = [bool]$known; Flags = [int]$flags
    Fallback = [bool]$known.AllowFallbackToUdp; Auto = $auto -and [bool]$known.AutoUpgrade
  }
})
ConvertTo-Json -Compress -InputObject $list
"#;

#[cfg(windows)]
fn judge_windows(servers: &[WindowsServer]) -> Resolution {
    if servers.is_empty() {
        return Resolution::Unknown;
    }
    let encrypted = |server: &WindowsServer| server.template && (server.flags != 0 || server.auto);
    if !servers.iter().all(encrypted) {
        return if servers
            .iter()
            .all(|server| server.address.starts_with("127.") || server.address == "::1")
        {
            Resolution::LocalResolver
        } else {
            Resolution::Plain
        };
    }
    if servers.iter().any(|server| server.fallback) {
        Resolution::Opportunistic
    } else {
        Resolution::Encrypted {
            protocol: Protocol::Https,
        }
    }
}

#[cfg(windows)]
fn system(_host: &str) -> Resolution {
    run(
        "powershell.exe",
        &["-NoProfile", "-NonInteractive", "-Command", WINDOWS_SCRIPT],
    )
    .and_then(|json| serde_json::from_str::<Vec<WindowsServer>>(json.trim()).ok())
    .map(|servers| judge_windows(&servers))
    .unwrap_or(Resolution::Unknown)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn needs_no_query_for_addresses_and_the_hosts_file() {
        assert_eq!(look("192.168.1.20"), Resolution::NoQuery);
        assert_eq!(look("[fe80::1]"), Resolution::NoQuery);
        assert!(hosts_file_has(
            "127.0.0.1 localhost\n192.168.1.20 nas nas.home # the NAS\n",
            "NAS.home"
        ));
        assert!(!hosts_file_has("# 192.168.1.20 nas\n", "nas"));
        assert_eq!(look("diskstation.local"), Resolution::Multicast);
    }

    #[cfg(windows)]
    #[test]
    fn judges_windows_dns_over_https() {
        let server = |address: &str, template, flags, fallback| WindowsServer {
            address: address.into(),
            template,
            flags,
            fallback,
            auto: false,
        };
        assert_eq!(
            judge_windows(&[server("192.168.1.1", false, 0, false)]),
            Resolution::Plain
        );
        assert_eq!(
            judge_windows(&[server("9.9.9.9", true, 1, false)]),
            Resolution::Encrypted {
                protocol: Protocol::Https
            }
        );
        assert_eq!(
            judge_windows(&[server("9.9.9.9", true, 1, true)]),
            Resolution::Opportunistic
        );
        assert_eq!(
            judge_windows(&[
                server("9.9.9.9", true, 1, false),
                server("192.168.1.1", false, 0, false)
            ]),
            Resolution::Plain
        );
        let parsed: Vec<WindowsServer> = serde_json::from_str(
            r#"[{"Address":"192.168.1.1","Template":false,"Flags":0,"Fallback":false,"Auto":false}]"#,
        )
        .unwrap();
        assert_eq!(judge_windows(&parsed), Resolution::Plain);
    }

    #[cfg(unix)]
    #[test]
    fn reads_resolved_and_resolv_conf() {
        assert_eq!(
            name_servers("# x\nnameserver 127.0.0.53\nnameserver fe80::1%eth0\nsearch home\n"),
            vec![
                "127.0.0.53".parse::<IpAddr>().unwrap(),
                "fe80::1".parse().unwrap()
            ]
        );
        assert_eq!(
            answering_link("github.com: 140.82.121.3      -- link: wlp3s0\n").as_deref(),
            Some("wlp3s0")
        );
        assert_eq!(
            dot_mode("Link 3 (wlp3s0): yes\n"),
            Some(Resolution::Encrypted {
                protocol: Protocol::Tls
            })
        );
        assert_eq!(
            dot_mode("Link 3 (wlp3s0): opportunistic\n"),
            Some(Resolution::Opportunistic)
        );
        assert_eq!(dot_mode("Global: no\n"), Some(Resolution::Plain));
    }
}
