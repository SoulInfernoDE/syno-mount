//! Certificate checks for connections to a NAS.
//!
//! Most NAS use DSM's self-signed certificate, which no system trusts. Instead of
//! turning verification off, the first connection shows the certificate's SHA-256
//! fingerprint to the user; once they trust it, it is pinned and every later
//! connection must present exactly that certificate. Certificates that the system
//! trusts (for example Let's Encrypt on DSM) pass without a pin.

use std::sync::{Arc, Mutex};

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{CryptoProvider, verify_tls12_signature, verify_tls13_signature};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};
use sha2::{Digest, Sha256};

/// How the certificate of the last handshake was judged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Verdict {
    /// Trusted by the system or matching the pin.
    Trusted,
    /// Not trusted by the system and no pin given.
    Unknown { fingerprint: String },
    /// A pin was given and the certificate is neither the pinned one nor trusted by the system.
    Changed { fingerprint: String },
}

/// SHA-256 of the DER certificate as colon-separated upper-case hex, as browsers show it.
pub fn fingerprint(certificate: &[u8]) -> String {
    Sha256::digest(certificate)
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join(":")
}

/// Where the verifier leaves its judgement of the last handshake.
pub(crate) type VerdictSlot = Arc<Mutex<Option<Verdict>>>;

#[derive(Debug)]
pub(crate) struct PinningVerifier {
    system: Arc<dyn ServerCertVerifier>,
    provider: Arc<CryptoProvider>,
    pinned: Option<String>,
    verdict: VerdictSlot,
}

impl PinningVerifier {
    fn record(&self, verdict: Verdict) {
        if let Ok(mut slot) = self.verdict.lock() {
            *slot = Some(verdict);
        }
    }
}

impl ServerCertVerifier for PinningVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let presented = fingerprint(end_entity);
        if self.pinned.as_deref() == Some(presented.as_str()) {
            self.record(Verdict::Trusted);
            return Ok(ServerCertVerified::assertion());
        }
        match self.system.verify_server_cert(
            end_entity,
            intermediates,
            server_name,
            ocsp_response,
            now,
        ) {
            Ok(verified) => {
                self.record(Verdict::Trusted);
                Ok(verified)
            }
            Err(error) => {
                let fingerprint = presented;
                self.record(if self.pinned.is_some() {
                    Verdict::Changed { fingerprint }
                } else {
                    Verdict::Unknown { fingerprint }
                });
                Err(error)
            }
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// A rustls configuration that checks certificates with the system and the pin,
/// and the slot that tells why the last handshake failed.
pub(crate) fn client_config(
    pinned: Option<String>,
) -> Result<(rustls::ClientConfig, VerdictSlot), rustls::Error> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let system = Arc::new(rustls_platform_verifier::Verifier::new(provider.clone())?);
    let verdict = Arc::new(Mutex::new(None));
    let verifier = PinningVerifier {
        system,
        provider: provider.clone(),
        pinned,
        verdict: verdict.clone(),
    };
    let config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(verifier))
        .with_no_client_auth();
    Ok((config, verdict))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_is_colon_separated_upper_hex() {
        let print = fingerprint(b"certificate");
        assert_eq!(print.len(), 32 * 3 - 1);
        assert!(print.split(':').all(|pair| {
            pair.len() == 2
                && pair
                    .chars()
                    .all(|c| c.is_ascii_digit() || c.is_ascii_uppercase())
        }));
    }
}
