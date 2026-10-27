//! Finds Synology NAS on the local network through mDNS/Bonjour.
//!
//! DSM announces its web interface as `_http._tcp` with TXT records such as
//! `vendor=Synology`, `model=DS923+`, `version_major=7` and `secure_admin_port=5001`
//! (Control Panel › File Services › Advanced › Bonjour). NAS without Bonjour are
//! found with Synology Assistant's own protocol (see `findhost`).

use std::net::Ipv4Addr;
use std::time::{Duration, Instant};

use mdns_sd::{ServiceDaemon, ServiceEvent};

mod findhost;
use serde::Serialize;

const SERVICE: &str = "_http._tcp.local.";
const DEFAULT_HTTPS_PORT: u16 = 5001;

/// A NAS that answered on the local network.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FoundNas {
    /// The server name set in DSM.
    pub name: String,
    pub model: String,
    /// IPv4 address, or the mDNS host name when none was announced.
    pub host: String,
    /// HTTPS port of DSM.
    pub port: u16,
    /// "7.3" when DSM announces its version.
    pub dsm: Option<String>,
}

/// Turns one announced service into a NAS, or `None` when it is not a Synology one.
fn from_record<'a>(
    fullname: &str,
    hostname: &str,
    addresses: &[Ipv4Addr],
    txt: impl Fn(&str) -> Option<&'a str>,
) -> Option<FoundNas> {
    if !txt("vendor")?.eq_ignore_ascii_case("synology") {
        return None;
    }
    let name = fullname
        .strip_suffix(&format!(".{SERVICE}"))
        .unwrap_or(fullname);
    let host = addresses
        .iter()
        .min()
        .map(ToString::to_string)
        .unwrap_or_else(|| hostname.trim_end_matches('.').to_owned());
    let dsm = match (txt("version_major"), txt("version_minor")) {
        (Some(major), Some(minor)) => Some(format!("{major}.{minor}")),
        (Some(major), None) => Some(major.to_owned()),
        _ => None,
    };
    Some(FoundNas {
        name: name.replace("\\032", " "),
        model: txt("model").unwrap_or_default().to_owned(),
        host,
        port: txt("secure_admin_port")
            .and_then(|port| port.parse().ok())
            .unwrap_or(DEFAULT_HTTPS_PORT),
        dsm,
    })
}

/// NAS that answered on Bonjour within `duration`.
fn browse_mdns(duration: Duration) -> Result<Vec<FoundNas>, String> {
    let daemon = ServiceDaemon::new().map_err(|error| error.to_string())?;
    let events = daemon.browse(SERVICE).map_err(|error| error.to_string())?;
    let deadline = Instant::now() + duration;

    let mut found: Vec<FoundNas> = Vec::new();
    while let Ok(event) = events.recv_deadline(deadline) {
        let ServiceEvent::ServiceResolved(service) = event else {
            continue;
        };
        let addresses: Vec<Ipv4Addr> = service.get_addresses_v4().into_iter().collect();
        let nas = from_record(
            service.get_fullname(),
            service.get_hostname(),
            &addresses,
            |key| service.get_property_val_str(key),
        );
        if let Some(nas) = nas.filter(|nas| found.iter().all(|known| known.host != nas.host)) {
            found.push(nas);
        }
    }
    let _ = daemon.shutdown();
    Ok(found)
}

/// Adds the NAS that answered Synology Assistant's query and completes the ones
/// Bonjour already found.
fn merge(found: &mut Vec<FoundNas>, answers: Vec<findhost::Answer>) {
    for answer in answers {
        let Some(ip) = answer.ip else { continue };
        let host = ip.to_string();
        if let Some(known) = found.iter_mut().find(|known| known.host == host) {
            if known.model.is_empty() {
                known.model = answer.model.unwrap_or_default();
            }
            // findhostd knows the full version ("7.4.1"), Bonjour only "7.4".
            if answer.dsm_version.is_some() {
                known.dsm = answer.dsm_version;
            }
            continue;
        }
        found.push(FoundNas {
            name: answer.hostname.unwrap_or_else(|| host.clone()),
            model: answer.model.unwrap_or_default(),
            port: answer.https_port.unwrap_or(DEFAULT_HTTPS_PORT),
            dsm: answer.dsm_version,
            host,
        });
    }
}

/// Searches with Bonjour and with Synology Assistant's protocol at the same time
/// and returns every NAS that answered within `duration`, by name. Fails only
/// when neither search could run.
pub fn browse(duration: Duration) -> Result<Vec<FoundNas>, String> {
    let assistant = std::thread::spawn(move || findhost::search(duration));
    let bonjour = browse_mdns(duration);
    let assistant = assistant
        .join()
        .map_err(|_| "the search stopped unexpectedly".to_owned())
        .and_then(|result| result.map_err(|error| error.to_string()));

    let mut found = match (bonjour, assistant) {
        (Err(bonjour), Err(assistant)) => return Err(format!("{bonjour}; {assistant}")),
        (Ok(found), Err(_)) => found,
        (Err(_), Ok(answers)) => {
            let mut found = Vec::new();
            merge(&mut found, answers);
            found
        }
        (Ok(mut found), Ok(answers)) => {
            merge(&mut found, answers);
            found
        }
    };
    found.sort_by_key(|nas| nas.name.to_lowercase());
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn txt<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<&'a str> {
        move |key| {
            pairs
                .iter()
                .find(|(name, _)| *name == key)
                .map(|(_, value)| *value)
        }
    }

    #[test]
    fn reads_a_synology_announcement() {
        let pairs = [
            ("vendor", "Synology"),
            ("model", "DS923+"),
            ("version_major", "7"),
            ("version_minor", "3"),
            ("secure_admin_port", "5551"),
        ];
        let nas = from_record(
            "nas-home._http._tcp.local.",
            "nas-home.local.",
            &[Ipv4Addr::new(192, 168, 1, 20)],
            txt(&pairs),
        )
        .unwrap();
        assert_eq!(
            nas,
            FoundNas {
                name: "nas-home".into(),
                model: "DS923+".into(),
                host: "192.168.1.20".into(),
                port: 5551,
                dsm: Some("7.3".into()),
            }
        );
    }

    #[test]
    fn assistant_answers_add_and_complete_nas() {
        let mut found = vec![FoundNas {
            name: "nas-home".into(),
            model: String::new(),
            host: "192.168.1.20".into(),
            port: 5001,
            dsm: Some("7.4".into()),
        }];
        merge(
            &mut found,
            vec![
                findhost::Answer {
                    ip: Some(Ipv4Addr::new(192, 168, 1, 20)),
                    model: Some("DS923+".into()),
                    dsm_version: Some("7.4.1".into()),
                    ..Default::default()
                },
                findhost::Answer {
                    hostname: Some("vdsm".into()),
                    ip: Some(Ipv4Addr::new(192, 168, 1, 42)),
                    model: Some("VirtualDSM".into()),
                    https_port: Some(5551),
                    ..Default::default()
                },
            ],
        );
        assert_eq!(found[0].model, "DS923+");
        assert_eq!(found[0].dsm.as_deref(), Some("7.4.1"));
        assert_eq!(found[1].name, "vdsm");
        assert_eq!(found[1].port, 5551);
    }

    #[test]
    fn ignores_other_vendors_and_falls_back_to_host_name() {
        let printer = [("vendor", "HP")];
        assert!(from_record("p._http._tcp.local.", "p.local.", &[], txt(&printer)).is_none());

        let bare = [("vendor", "synology")];
        let nas = from_record("nas._http._tcp.local.", "nas.local.", &[], txt(&bare)).unwrap();
        assert_eq!(nas.host, "nas.local");
        assert_eq!(nas.port, DEFAULT_HTTPS_PORT);
    }
}
