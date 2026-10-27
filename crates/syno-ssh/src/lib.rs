//! SSH for Syno Mount: one ed25519 key per NAS and account, the NAS host key pinned
//! in the app's own `known_hosts`, and the key installed on the NAS over SFTP.
//!
//! Everything runs over SFTP, so it also works for accounts without shell access
//! (DSM gives a shell only to administrators). The user's own `~/.ssh` stays
//! untouched: keys and `known_hosts` live in the app's data folder and are handed to
//! `sshfs` as options.

mod keys;
mod remote;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh::client;
use russh::keys::{Algorithm, HashAlg, PrivateKey, PrivateKeyWithHashAlg, PublicKey};
use russh::{Preferred, client::Handle};
use russh_sftp::client::SftpSession;
use serde::Serialize;

pub use keys::{KeyPair, KnownHosts, host_alias};
pub use remote::{KEY_MARKER, Layout};

/// Time for TCP connect and SSH handshake together.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("cannot reach the SSH service: {0}")]
    Network(String),
    #[error("the NAS presented an unknown host key ({fingerprint})")]
    HostKeyUnknown { fingerprint: String },
    #[error("the host key of the NAS changed ({fingerprint})")]
    HostKeyChanged { fingerprint: String },
    #[error("the NAS refused the sign-in")]
    AuthFailed,
    #[error("SFTP: {0}")]
    Sftp(String),
    #[error("{0}")]
    Io(String),
}

impl Error {
    /// A stable, kebab-case key for the user interface to pick its message.
    pub fn key(&self) -> &'static str {
        match self {
            Self::Network(_) => "ssh-network",
            Self::HostKeyUnknown { .. } => "host-key-unknown",
            Self::HostKeyChanged { .. } => "host-key-changed",
            Self::AuthFailed => "ssh-auth-failed",
            Self::Sftp(_) => "sftp",
            Self::Io(_) => "io",
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.to_string())
    }
}

impl From<russh_sftp::client::error::Error> for Error {
    fn from(error: russh_sftp::client::error::Error) -> Self {
        Self::Sftp(error.to_string())
    }
}

/// The host key of a NAS as the user sees it before trusting it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostKey {
    /// "ssh-ed25519 AAAA…", without a comment.
    pub openssh: String,
    /// "ssh-ed25519"; handed to ssh as `HostKeyAlgorithms`.
    pub algorithm: String,
    /// "SHA256:…", as `ssh-keygen -l` prints it.
    pub fingerprint: String,
}

impl HostKey {
    pub fn from_public(key: &PublicKey) -> Self {
        let mut plain = key.clone();
        plain.set_comment("");
        Self {
            openssh: plain.to_openssh().unwrap_or_default().trim_end().to_owned(),
            algorithm: key.algorithm().as_str().to_owned(),
            fingerprint: key.fingerprint(HashAlg::Sha256).to_string(),
        }
    }

    fn public(&self) -> Option<PublicKey> {
        PublicKey::from_openssh(&self.openssh).ok()
    }
}

/// Where an SSH connection goes and whom it trusts.
#[derive(Debug, Clone)]
pub struct Target {
    pub host: String,
    pub port: u16,
    pub account: String,
    /// The pinned host key; `None` only for [`scan_host_key`].
    pub host_key: Option<HostKey>,
}

struct Checker {
    expected: Option<PublicKey>,
    seen: Arc<Mutex<Option<PublicKey>>>,
}

impl client::Handler for Checker {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_key: &russh::keys::PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let russh::keys::PublicKeyOrCertificate::PublicKey { key, .. } = server_key else {
            return Ok(false);
        };
        if let Ok(mut seen) = self.seen.lock() {
            *seen = Some(key.clone());
        }
        Ok(self
            .expected
            .as_ref()
            .is_some_and(|expected| expected.key_data() == key.key_data()))
    }
}

fn config() -> Arc<client::Config> {
    Arc::new(client::Config {
        inactivity_timeout: Some(Duration::from_secs(30)),
        preferred: Preferred {
            // The same order OpenSSH uses; DSM has all three.
            key: std::borrow::Cow::Borrowed(&[
                Algorithm::Ed25519,
                Algorithm::Ecdsa {
                    curve: russh::keys::EcdsaCurve::NistP256,
                },
            ]),
            ..Preferred::default()
        },
        ..client::Config::default()
    })
}

/// Opens an SSH connection that only goes on when the host key matches the pinned one.
async fn open(target: &Target) -> Result<Handle<Checker>, Error> {
    let expected = target.host_key.as_ref().and_then(HostKey::public);
    let seen = Arc::new(Mutex::new(None));
    let checker = Checker {
        expected: expected.clone(),
        seen: seen.clone(),
    };
    let address = (target.host.as_str(), target.port);
    let connected =
        tokio::time::timeout(CONNECT_TIMEOUT, client::connect(config(), address, checker)).await;
    let seen = seen.lock().ok().and_then(|mut seen| seen.take());
    match connected {
        Err(_) => Err(Error::Network("timed out".into())),
        Ok(Ok(handle)) => Ok(handle),
        Ok(Err(error)) => Err(match (seen, expected) {
            (Some(key), None) => Error::HostKeyUnknown {
                fingerprint: HostKey::from_public(&key).fingerprint,
            },
            (Some(key), Some(expected)) if key.key_data() != expected.key_data() => {
                Error::HostKeyChanged {
                    fingerprint: HostKey::from_public(&key).fingerprint,
                }
            }
            _ => Error::Network(error.to_string()),
        }),
    }
}

/// Reads the host key of the SSH service without signing in.
pub async fn scan_host_key(host: &str, port: u16) -> Result<HostKey, Error> {
    let seen = Arc::new(Mutex::new(None));
    let checker = Checker {
        expected: None,
        seen: seen.clone(),
    };
    let connected = tokio::time::timeout(
        CONNECT_TIMEOUT,
        client::connect(config(), (host, port), checker),
    )
    .await;
    if let Some(key) = seen.lock().ok().and_then(|mut seen| seen.take()) {
        return Ok(HostKey::from_public(&key));
    }
    Err(match connected {
        Err(_) => Error::Network("timed out".into()),
        Ok(Err(error)) => Error::Network(error.to_string()),
        Ok(Ok(_)) => Error::Network("no host key".into()),
    })
}

/// The ways to sign in over SSH.
pub enum Login<'a> {
    Password(&'a str),
    Key(&'a PrivateKey),
}

/// A signed-in SFTP session.
pub struct Sftp {
    pub(crate) session: SftpSession,
    // Keeps the SSH connection open as long as the SFTP session.
    _handle: Handle<Checker>,
}

/// Signs in and opens SFTP.
pub async fn connect(target: &Target, login: Login<'_>) -> Result<Sftp, Error> {
    let mut handle = open(target).await?;
    let result = match login {
        Login::Password(password) => {
            handle
                .authenticate_password(target.account.clone(), password)
                .await
        }
        Login::Key(key) => {
            let key = PrivateKeyWithHashAlg::new(Arc::new(key.clone()), None);
            handle
                .authenticate_publickey(target.account.clone(), key)
                .await
        }
    }
    .map_err(|error| Error::Network(error.to_string()))?;
    if !result.success() {
        return Err(Error::AuthFailed);
    }
    let channel = handle
        .channel_open_session()
        .await
        .map_err(|error| Error::Sftp(error.to_string()))?;
    channel
        .request_subsystem(true, "sftp")
        .await
        .map_err(|error| Error::Sftp(error.to_string()))?;
    let session = SftpSession::new(channel.into_stream()).await?;
    Ok(Sftp {
        session,
        _handle: handle,
    })
}
