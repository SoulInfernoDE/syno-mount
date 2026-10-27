//! The discovery protocol of Synology Assistant, answered by DSM's `findhostd`.
//! It finds NAS that do not announce themselves through Bonjour.
//!
//! A packet is a magic header followed by TLV records: one byte ID, one byte
//! length, the value. The client broadcasts a query to UDP port 9999; every NAS
//! answers on port 9999, once as broadcast and once directly. The layout follows
//! public analyses of `findhostd` and 007revad's `syno_discover.py` (MIT).

use std::net::{Ipv4Addr, SocketAddrV4, UdpSocket};
use std::time::{Duration, Instant};

use socket2::{Domain, Protocol, Socket, Type};

const MAGIC: &[u8; 8] = b"\x12\x34\x56\x78SYNO";
const PORT: u16 = 9999;

/// The sender's MAC address in a query: a locally administered address that
/// belongs to no vendor (findhostd ignores queries that seem to come from a
/// Synology device), new for every query, because findhostd stays silent for a
/// while towards a sender it has just answered.
fn query_mac() -> Vec<u8> {
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |time| time.as_nanos())
        ^ u128::from(std::process::id());
    let bytes = seed.to_le_bytes();
    format!(
        "02:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4]
    )
    .into_bytes()
}

const ID_TYPE: u8 = 0x01;
const ID_HOSTNAME: u8 = 0x11;
const ID_IP: u8 = 0x12;
const ID_MAC: u8 = 0x19;
const ID_DSM_BUILD: u8 = 0x49;
const ID_HTTP_PORT: u8 = 0x75;
const ID_HTTPS_PORT: u8 = 0x76;
const ID_DSM_VERSION: u8 = 0x77;
const ID_MODEL: u8 = 0x78;

/// What a NAS tells about itself. The serial number it also sends is not kept.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Answer {
    pub hostname: Option<String>,
    pub ip: Option<Ipv4Addr>,
    pub mac: Option<String>,
    pub model: Option<String>,
    pub dsm_version: Option<String>,
    pub dsm_build: Option<u32>,
    pub http_port: Option<u16>,
    pub https_port: Option<u16>,
}

fn tlv(packet: &mut Vec<u8>, id: u8, value: &[u8]) {
    let length = value.len().min(255);
    packet.push(id);
    packet.push(length as u8);
    packet.extend_from_slice(&value[..length]);
}

/// The broadcast query (packet type 1) with the sender MAC `mac`.
pub(crate) fn query(mac: &[u8]) -> Vec<u8> {
    let mut packet = MAGIC.to_vec();
    tlv(&mut packet, 0xa4, &[0x00, 0x00, 0x02, 0x01]);
    tlv(&mut packet, 0xa6, &[0x78, 0x00, 0x00, 0x00]);
    tlv(&mut packet, ID_TYPE, &1u32.to_le_bytes());
    tlv(&mut packet, 0xb0, &[0xc0, 0x01, 0, 0, 0, 0, 0, 0]);
    tlv(&mut packet, 0xb1, &[0; 8]);
    tlv(&mut packet, 0xb8, &[0xc0, 0x01, 0, 0, 0, 0, 0, 0]);
    tlv(&mut packet, 0xb9, &[0; 8]);
    for _ in 0..4 {
        tlv(&mut packet, 0x7c, mac);
    }
    packet
}

/// Printable ASCII only; DSM pads some strings with control bytes.
fn text(value: &[u8]) -> Option<String> {
    let text: String = String::from_utf8_lossy(value)
        .chars()
        .filter(|c| !c.is_control() && *c != '\u{fffd}')
        .collect::<String>()
        .trim()
        .to_owned();
    (!text.is_empty()).then_some(text)
}

fn le_u32(value: &[u8]) -> Option<u32> {
    Some(u32::from_le_bytes(value.try_into().ok()?))
}

/// Reads an answer; `None` for anything that is not a findhostd packet or is the
/// query itself (which comes back on the broadcast address, too).
pub(crate) fn parse(packet: &[u8]) -> Option<Answer> {
    let mut rest = packet.strip_prefix(MAGIC.as_slice())?;
    let mut answer = Answer::default();
    while let [id, length, tail @ ..] = rest {
        let length = usize::from(*length);
        if tail.len() < length {
            break;
        }
        let (value, next) = tail.split_at(length);
        rest = next;
        match *id {
            ID_HOSTNAME => answer.hostname = text(value),
            ID_IP if value.len() == 4 => {
                answer.ip = Some(Ipv4Addr::new(value[0], value[1], value[2], value[3]));
            }
            ID_MAC => answer.mac = text(value),
            ID_MODEL => answer.model = text(value),
            ID_DSM_VERSION => answer.dsm_version = text(value),
            ID_DSM_BUILD => answer.dsm_build = le_u32(value),
            ID_HTTP_PORT => answer.http_port = le_u32(value).and_then(|p| u16::try_from(p).ok()),
            ID_HTTPS_PORT => {
                answer.https_port = le_u32(value).and_then(|p| u16::try_from(p).ok());
            }
            _ => {}
        }
    }
    (answer.hostname.is_some() || answer.model.is_some()).then_some(answer)
}

/// A UDP socket that may send broadcasts and receives on port 9999, shared with
/// Synology Assistant if it runs, too. Without port 9999 only direct answers arrive.
fn socket() -> std::io::Result<UdpSocket> {
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    socket.set_reuse_address(true)?;
    #[cfg(all(unix, not(target_os = "solaris")))]
    socket.set_reuse_port(true)?;
    socket.set_broadcast(true)?;
    let any = |port| SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port).into();
    if socket.bind(&any(PORT)).is_err() {
        socket.bind(&any(0))?;
    }
    Ok(socket.into())
}

/// How often the query is sent within one search. UDP may lose packets, and a
/// NAS often drops its first direct answer while it looks up this computer's
/// address on the network; Synology Assistant repeats its query, too.
const SENDS: u32 = 3;

/// Broadcasts the query and collects the answers for `duration`, one per NAS.
pub(crate) fn search(duration: Duration) -> std::io::Result<Vec<Answer>> {
    let socket = socket()?;
    let target = SocketAddrV4::new(Ipv4Addr::BROADCAST, PORT);

    let start = Instant::now();
    let deadline = start + duration;
    let interval = duration / (SENDS + 1);
    let mut sent = 0;
    let mut answers: Vec<Answer> = Vec::new();
    let mut buffer = [0u8; 4096];
    loop {
        let now = Instant::now();
        if now >= deadline {
            break;
        }
        let next_send = start + interval * sent;
        if sent < SENDS && now >= next_send {
            socket.send_to(&query(&query_mac()), target)?;
            sent += 1;
            continue;
        }
        let wake = if sent < SENDS {
            (start + interval * sent).min(deadline)
        } else {
            deadline
        };
        let wait = wake
            .saturating_duration_since(now)
            .max(Duration::from_millis(1));
        socket.set_read_timeout(Some(wait))?;
        let (length, from) = match socket.recv_from(&mut buffer) {
            Ok(received) => received,
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                continue;
            }
            Err(error) => return Err(error),
        };
        let Some(mut answer) = parse(&buffer[..length]) else {
            continue;
        };
        if answer.ip.is_none()
            && let std::net::IpAddr::V4(ip) = from.ip()
        {
            answer.ip = Some(ip);
        }
        // Every NAS answers twice (broadcast and direct), once per network port.
        let duplicate = answers
            .iter()
            .any(|known| known.ip == answer.ip || (known.mac.is_some() && known.mac == answer.mac));
        if !duplicate {
            answers.push(answer);
        }
    }
    Ok(answers)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_has_magic_type_one_and_four_sender_macs() {
        let mac = query_mac();
        assert_eq!(mac.len(), 17);
        assert!(mac.starts_with(b"02:"), "locally administered");
        let packet = query(&mac);
        assert!(packet.starts_with(MAGIC));
        let type_record = [ID_TYPE, 4, 1, 0, 0, 0];
        assert!(packet.windows(6).any(|window| window == type_record));
        let macs = packet
            .windows(2 + mac.len())
            .filter(|window| window[0] == 0x7c && window[2..] == mac[..])
            .count();
        assert_eq!(macs, 4);
    }

    #[test]
    fn reads_an_answer() {
        let mut packet = MAGIC.to_vec();
        tlv(&mut packet, ID_TYPE, &2u32.to_le_bytes());
        tlv(&mut packet, ID_HOSTNAME, b"nas-home");
        tlv(&mut packet, ID_IP, &[192, 168, 1, 20]);
        tlv(&mut packet, ID_MAC, b"00:11:32:aa:bb:cc");
        tlv(&mut packet, ID_MODEL, b"DS923+");
        tlv(&mut packet, ID_DSM_VERSION, b"7.4.1\x90");
        tlv(&mut packet, ID_DSM_BUILD, &90080u32.to_le_bytes());
        tlv(&mut packet, ID_HTTPS_PORT, &5551u32.to_le_bytes());
        tlv(&mut packet, 0xc0, b"SERIAL123");
        let answer = parse(&packet).unwrap();
        assert_eq!(answer.hostname.as_deref(), Some("nas-home"));
        assert_eq!(answer.ip, Some(Ipv4Addr::new(192, 168, 1, 20)));
        assert_eq!(answer.model.as_deref(), Some("DS923+"));
        assert_eq!(answer.dsm_version.as_deref(), Some("7.4.1"));
        assert_eq!(answer.dsm_build, Some(90080));
        assert_eq!(answer.https_port, Some(5551));
    }

    #[test]
    fn ignores_foreign_packets_and_the_own_query() {
        assert_eq!(parse(b"hello"), None);
        assert_eq!(parse(&query(&query_mac())), None);
        let mut cut = MAGIC.to_vec();
        cut.extend_from_slice(&[ID_HOSTNAME, 20, b'x']);
        assert_eq!(parse(&cut), None);
    }
}
