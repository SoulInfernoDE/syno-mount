//! SSH inside TLS, for a NAS whose only door to the internet is a reverse proxy on
//! port 443. The proxy recognises the connection by its ALPN protocol (or by the
//! server name), ends TLS and passes the bytes on to the SSH port of the NAS. SSH
//! runs end to end inside, so the pinned host key stays the trust anchor; TLS is
//! checked as well, against the system's certificates or a pinned fingerprint.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::WebPkiSupportedAlgorithms;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, SignatureScheme};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio_rustls::TlsConnector;
use tokio_rustls::client::TlsStream;

use crate::Error;

/// The ALPN protocol the proxy routes on.
pub const ALPN: &[u8] = b"syno-mount-ssh";

/// Large buffers keep the copying out of the way of SFTP's big reads and writes.
const BUFFER: usize = 256 * 1024;

/// How an address is reached through a reverse proxy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tunnel {
    /// The name in the TLS hello: the proxy's name, or a dedicated one.
    pub server_name: String,
    /// Offer the ALPN protocol the proxy routes on; off for routing by name alone.
    #[serde(default = "yes")]
    pub alpn: bool,
    /// SHA-256 of a self-signed proxy certificate the user confirmed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pinned_certificate: Option<String>,
}

fn yes() -> bool {
    true
}

impl Tunnel {
    /// A tunnel to the proxy at `host`, routed by ALPN.
    pub fn new(host: &str) -> Self {
        Self {
            server_name: host.to_owned(),
            alpn: true,
            pinned_certificate: None,
        }
    }
}

/// "SHA256:ab:cd:…" of a certificate, as browsers show it.
pub fn fingerprint(certificate: &[u8]) -> String {
    let digest = Sha256::digest(certificate);
    let hex: Vec<String> = digest.iter().map(|byte| format!("{byte:02X}")).collect();
    format!("SHA256:{}", hex.join(":"))
}

/// Checks the proxy's certificate and remembers it, so that a refusal can say
/// which certificate it was.
#[derive(Debug)]
struct Checker {
    pinned: Option<String>,
    system: Option<Arc<dyn ServerCertVerifier>>,
    algorithms: WebPkiSupportedAlgorithms,
    seen: Arc<Mutex<Option<String>>>,
}

impl ServerCertVerifier for Checker {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let seen = fingerprint(end_entity);
        if let Ok(mut slot) = self.seen.lock() {
            *slot = Some(seen.clone());
        }
        match (&self.pinned, &self.system) {
            (Some(pinned), _) if *pinned == seen => Ok(ServerCertVerified::assertion()),
            (Some(_), _) | (None, None) => Err(rustls::Error::InvalidCertificate(
                rustls::CertificateError::ApplicationVerificationFailure,
            )),
            (None, Some(system)) => system.verify_server_cert(
                end_entity,
                intermediates,
                server_name,
                ocsp_response,
                now,
            ),
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(message, cert, dss, &self.algorithms)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &self.algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.algorithms.supported_schemes()
    }
}

struct Prepared {
    config: Arc<ClientConfig>,
    server_name: ServerName<'static>,
    seen: Arc<Mutex<Option<String>>>,
    pinned: Option<String>,
}

fn prepare(tunnel: &Tunnel) -> Result<Prepared, Error> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let system: Option<Arc<dyn ServerCertVerifier>> = if tunnel.pinned_certificate.is_some() {
        None
    } else {
        Some(Arc::new(
            rustls_platform_verifier::Verifier::new(provider.clone())
                .map_err(|error| Error::Tls(error.to_string()))?,
        ))
    };
    let seen = Arc::new(Mutex::new(None));
    let checker = Checker {
        pinned: tunnel.pinned_certificate.clone(),
        system,
        algorithms: provider.signature_verification_algorithms,
        seen: seen.clone(),
    };
    let mut config = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|error| Error::Tls(error.to_string()))?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(checker))
        .with_no_client_auth();
    if tunnel.alpn {
        config.alpn_protocols = vec![ALPN.to_vec()];
    }
    let server_name = ServerName::try_from(tunnel.server_name.clone())
        .map_err(|_| Error::Tls(format!("not a server name: {}", tunnel.server_name)))?;
    Ok(Prepared {
        config: Arc::new(config),
        server_name,
        seen,
        pinned: tunnel.pinned_certificate.clone(),
    })
}

/// Turns a failed handshake into an error that names the certificate.
fn handshake_error(prepared: &Prepared, error: &std::io::Error) -> Error {
    let refused = error
        .get_ref()
        .and_then(|inner| inner.downcast_ref::<rustls::Error>())
        .is_some_and(|tls| matches!(tls, rustls::Error::InvalidCertificate(_)));
    let seen = prepared.seen.lock().ok().and_then(|seen| seen.clone());
    match (refused, seen) {
        (true, Some(fingerprint)) if prepared.pinned.is_some() => {
            Error::CertificateChanged { fingerprint }
        }
        (true, Some(fingerprint)) => Error::CertificateUntrusted { fingerprint },
        _ => Error::Tls(error.to_string()),
    }
}

/// Opens TLS to the proxy, for russh and for `syno-mount tunnel`.
pub async fn connect(
    host: &str,
    port: u16,
    tunnel: &Tunnel,
) -> Result<TlsStream<tokio::net::TcpStream>, Error> {
    let prepared = prepare(tunnel)?;
    let tcp = tokio::net::TcpStream::connect((host, port))
        .await
        .map_err(|error| Error::Network(error.to_string()))?;
    let _ = tcp.set_nodelay(true);
    TlsConnector::from(prepared.config.clone())
        .connect(prepared.server_name.clone(), tcp)
        .await
        .map_err(|error| handshake_error(&prepared, &error))
}

/// Connects stdin and stdout to the SSH service behind the proxy: the
/// `ProxyCommand` of ssh. Returns when either side closes.
pub async fn pipe_stdio(host: &str, port: u16, tunnel: &Tunnel) -> Result<(), Error> {
    let mut tls = connect(host, port, tunnel).await?;
    let mut stdio = tokio::io::join(tokio::io::stdin(), tokio::io::stdout());
    tokio::io::copy_bidirectional_with_sizes(&mut stdio, &mut tls, BUFFER, BUFFER)
        .await
        .map(|_| ())
        .map_err(|error| Error::Network(error.to_string()))
}

/// Connects through the proxy and waits for the SSH banner of the NAS. Returns the
/// time the TCP connection took, which is what the status page shows as latency.
pub fn probe(host: &str, port: u16, tunnel: &Tunnel, timeout: Duration) -> Result<Duration, Error> {
    let prepared = prepare(tunnel)?;
    let targets = (host, port)
        .to_socket_addrs()
        .map_err(|error| Error::Network(error.to_string()))?;
    let mut last = Error::Network(format!("{host} has no address"));
    for target in targets {
        let started = Instant::now();
        let tcp = match TcpStream::connect_timeout(&target, timeout) {
            Ok(tcp) => tcp,
            Err(error) => {
                last = Error::Network(error.to_string());
                continue;
            }
        };
        let latency = started.elapsed();
        tcp.set_read_timeout(Some(timeout))?;
        tcp.set_write_timeout(Some(timeout))?;
        let connection =
            rustls::ClientConnection::new(prepared.config.clone(), prepared.server_name.clone())
                .map_err(|error| Error::Tls(error.to_string()))?;
        let mut tls = rustls::StreamOwned::new(connection, tcp);
        let mut banner = [0u8; 8];
        let mut read = 0;
        while read < 4 {
            match tls.read(&mut banner[read..]) {
                Ok(0) => return Err(Error::NoSshBehindProxy),
                Ok(count) => read += count,
                Err(error) if error.kind() == std::io::ErrorKind::InvalidData => {
                    return Err(handshake_error(&prepared, &error));
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) && tls.conn.is_handshaking() =>
                {
                    return Err(Error::Network("the proxy does not answer".into()));
                }
                Err(_) if !tls.conn.is_handshaking() => return Err(Error::NoSshBehindProxy),
                Err(error) => return Err(Error::Network(error.to_string())),
            }
        }
        if !banner.starts_with(b"SSH-") {
            return Err(Error::NoSshBehindProxy);
        }
        // Say goodbye properly, so that the proxy's log does not show an error.
        tls.conn.send_close_notify();
        let _ = tls.flush();
        return Ok(latency);
    }
    Err(last)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprints_like_browsers() {
        let print = fingerprint(b"abc");
        assert!(print.starts_with("SHA256:BA:78:16:BF:"));
        assert_eq!(print.len(), "SHA256:".len() + 32 * 3 - 1);
    }

    #[test]
    fn reads_old_and_short_settings() {
        let tunnel: Tunnel = serde_json::from_str(r#"{"serverName":"nas.example.org"}"#).unwrap();
        assert_eq!(tunnel, Tunnel::new("nas.example.org"));
        let text = serde_json::to_string(&tunnel).unwrap();
        assert!(!text.contains("pinnedCertificate"));
    }

    #[test]
    fn offers_the_alpn_protocol_only_when_asked() {
        let mut tunnel = Tunnel::new("nas.example.org");
        assert_eq!(
            prepare(&tunnel).unwrap().config.alpn_protocols,
            vec![ALPN.to_vec()]
        );
        tunnel.alpn = false;
        assert!(prepare(&tunnel).unwrap().config.alpn_protocols.is_empty());
    }

    #[test]
    fn refuses_names_that_are_no_server_names() {
        assert!(prepare(&Tunnel::new("not a name")).is_err());
    }
}
