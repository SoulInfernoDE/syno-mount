//! Client for the DSM Web API of Synology NAS.
//!
//! Taken from Syno Companion's `companion-dsm` (same authors, same licence) and
//! extended with the calls Syno Mount needs (see [`files`]); both go back into one
//! crate when Syno Companion takes over the mounts.
//!
//! Every request goes over HTTPS as a form POST, so passwords and session IDs never
//! appear in URLs or server logs. Certificates are checked by the system or pinned
//! after the user trusted them (see [`tls`]).

mod cpu;
mod files;
mod overview;
mod setup;
pub mod tls;

use std::collections::HashMap;
use std::time::Duration;

use serde::Deserialize;
use serde_json::Value;
use url::Url;

pub use files::{CpuKind, Services, Share};
pub use overview::{Drive, DriveKind, Health, NetworkPort, Overview, Volume};
pub use setup::{
    PasswordRules, SetupDone, SetupInfo, SetupRequest, SetupState, TimeZone, UpdatePolicy,
};
use tls::Verdict;

/// Session name sent to DSM; it shows up in DSM's list of signed-in connections.
const SESSION_NAME: &str = "SynoMount";

/// APIs this client uses, looked up once per sign-in.
const APIS: &[&str] = &[
    "SYNO.API.Auth",
    "SYNO.Core.FileServ.FTP.SFTP",
    "SYNO.Core.Network",
    "SYNO.Core.Network.Ethernet",
    "SYNO.Core.System",
    "SYNO.Core.System.Utilization",
    "SYNO.Core.User.Home",
    "SYNO.FileStation.List",
    "SYNO.Storage.CGI.Storage",
];

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("cannot reach the NAS: {0}")]
    Network(String),
    #[error("the NAS presented a certificate that is not trusted yet ({fingerprint})")]
    UntrustedCertificate { fingerprint: String },
    #[error("the certificate of the NAS changed ({fingerprint})")]
    CertificateChanged { fingerprint: String },
    #[error("sign-in failed: {0:?}")]
    Auth(AuthError),
    #[error("the session has ended")]
    SessionExpired,
    #[error("this account may not use {0}")]
    PermissionDenied(String),
    #[error("DSM reported error {code} for {api}")]
    Api { api: String, code: i64 },
    #[error("unexpected response from the NAS: {0}")]
    Unexpected(String),
}

/// Why DSM refused a sign-in (codes of `SYNO.API.Auth`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthError {
    WrongCredentials,
    AccountDisabled,
    PermissionDenied,
    OtpRequired,
    OtpWrong,
    OtpEnforced,
    IpBlocked,
    PasswordExpired,
    Other(i64),
}

impl AuthError {
    fn from_code(code: i64) -> Self {
        match code {
            400 => Self::WrongCredentials,
            401 => Self::AccountDisabled,
            402 => Self::PermissionDenied,
            403 => Self::OtpRequired,
            404 => Self::OtpWrong,
            406 => Self::OtpEnforced,
            407 => Self::IpBlocked,
            408..=410 => Self::PasswordExpired,
            other => Self::Other(other),
        }
    }
}

impl Error {
    /// A stable, kebab-case key for the user interface to pick its message.
    pub fn key(&self) -> &'static str {
        match self {
            Self::Network(_) => "network",
            Self::UntrustedCertificate { .. } => "certificate-untrusted",
            Self::CertificateChanged { .. } => "certificate-changed",
            Self::Auth(AuthError::WrongCredentials) => "wrong-credentials",
            Self::Auth(AuthError::AccountDisabled) => "account-disabled",
            Self::Auth(AuthError::PermissionDenied) => "account-not-allowed",
            Self::Auth(AuthError::OtpRequired) => "otp-required",
            Self::Auth(AuthError::OtpWrong) => "otp-wrong",
            Self::Auth(AuthError::OtpEnforced) => "otp-enforced",
            Self::Auth(AuthError::IpBlocked) => "ip-blocked",
            Self::Auth(AuthError::PasswordExpired) => "password-expired",
            Self::Auth(AuthError::Other(_)) => "sign-in-failed",
            Self::SessionExpired => "session-expired",
            Self::PermissionDenied(_) => "permission-denied",
            Self::Api { .. } | Self::Unexpected(_) => "unexpected",
        }
    }
}

/// What the user enters to sign in.
#[derive(Debug, Clone, Default)]
pub struct Credentials {
    pub account: String,
    pub password: String,
    /// The current code of the authenticator app, when DSM asked for one.
    pub otp: Option<String>,
    /// Token of a device DSM remembered earlier; skips the code.
    pub device_token: Option<String>,
    /// Ask DSM to remember this device, so the next sign-in needs no code.
    pub remember_device: bool,
    /// Shown in DSM's list of trusted devices.
    pub device_name: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ApiInfo {
    path: String,
    #[serde(rename = "maxVersion")]
    max_version: u32,
    #[serde(rename = "minVersion")]
    min_version: u32,
}

/// A signed-in session.
#[derive(Debug, Clone)]
pub struct Session {
    sid: String,
    /// Device token for the next sign-in, when DSM issued or accepted one.
    pub device_token: Option<String>,
    apis: HashMap<String, ApiInfo>,
}

/// Connection to one NAS.
#[derive(Debug, Clone)]
pub struct Client {
    http: reqwest::Client,
    base: Url,
    verdict: tls::VerdictSlot,
}

impl Client {
    /// `pinned` is the fingerprint of a certificate the user trusted before.
    pub fn new(host: &str, port: u16, pinned: Option<&str>) -> Result<Self, Error> {
        let host = if host.contains(':') && !host.starts_with('[') {
            format!("[{host}]") // IPv6 literal
        } else {
            host.to_owned()
        };
        let base = Url::parse(&format!("https://{host}:{port}/"))
            .map_err(|error| Error::Unexpected(format!("invalid address: {error}")))?;
        let (tls, verdict) = tls::client_config(pinned.map(str::to_owned))
            .map_err(|error| Error::Unexpected(format!("TLS setup: {error}")))?;
        let http = reqwest::Client::builder()
            .tls_backend_preconfigured(tls)
            // DSM's setup session arrives as a cookie.
            .cookie_store(true)
            .connect_timeout(Duration::from_secs(6))
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|error| Error::Unexpected(format!("HTTP setup: {error}")))?;
        Ok(Self {
            http,
            base,
            verdict,
        })
    }

    #[cfg(test)]
    fn with_base(base: Url) -> Self {
        // reqwest has no default TLS provider here, even for plain HTTP.
        let (tls, verdict) = tls::client_config(None).unwrap();
        let http = reqwest::Client::builder()
            .tls_backend_preconfigured(tls)
            .cookie_store(true)
            .build()
            .unwrap();
        Self {
            http,
            base,
            verdict,
        }
    }

    /// Turns a transport error into a certificate error when the handshake failed on it.
    fn transport_error(&self, error: &reqwest::Error) -> Error {
        let verdict = self.verdict.lock().ok().and_then(|mut slot| slot.take());
        match verdict {
            Some(Verdict::Unknown { fingerprint }) => Error::UntrustedCertificate { fingerprint },
            Some(Verdict::Changed { fingerprint }) => Error::CertificateChanged { fingerprint },
            _ => Error::Network(without_url(error)),
        }
    }

    /// One Web API call; returns the `data` member of a successful answer.
    async fn post(&self, path: &str, api: &str, form: &[(&str, &str)]) -> Result<Value, Error> {
        let url = self
            .base
            .join(&format!("webapi/{path}"))
            .map_err(|error| Error::Unexpected(error.to_string()))?;
        let response = self
            .http
            .post(url)
            .form(form)
            .send()
            .await
            .map_err(|error| self.transport_error(&error))?;
        let status = response.status();
        let body: Value = response
            .json()
            .await
            .map_err(|_| Error::Unexpected(format!("HTTP {status} without a DSM answer")))?;
        if body["success"].as_bool() == Some(true) {
            return Ok(body.get("data").cloned().unwrap_or(Value::Null));
        }
        let code = body["error"]["code"].as_i64().unwrap_or(-1);
        Err(match (api, code) {
            ("SYNO.API.Auth", 400..=499) => Error::Auth(AuthError::from_code(code)),
            (_, 106 | 107 | 119) => Error::SessionExpired,
            (_, 105) => Error::PermissionDenied(api.to_owned()),
            _ => Error::Api {
                api: api.to_owned(),
                code,
            },
        })
    }

    async fn api_info(&self) -> Result<HashMap<String, ApiInfo>, Error> {
        let query = APIS.join(",");
        let data = self
            .post(
                "query.cgi",
                "SYNO.API.Info",
                &[
                    ("api", "SYNO.API.Info"),
                    ("version", "1"),
                    ("method", "query"),
                    ("query", &query),
                ],
            )
            .await?;
        serde_json::from_value(data)
            .map_err(|error| Error::Unexpected(format!("API list: {error}")))
    }

    /// Signs in. `Err(Error::Auth(AuthError::OtpRequired))` means: ask for the
    /// authenticator code and call again with `otp` set.
    pub async fn login(&self, credentials: &Credentials) -> Result<Session, Error> {
        let apis = self.api_info().await?;
        let auth = apis
            .get("SYNO.API.Auth")
            .ok_or_else(|| Error::Unexpected("SYNO.API.Auth is missing".into()))?;
        // Version 6 brings device tokens; DSM 7 offers up to 7.
        let version = auth.max_version.min(6).max(auth.min_version).to_string();

        let mut form = vec![
            ("api", "SYNO.API.Auth"),
            ("version", version.as_str()),
            ("method", "login"),
            ("account", credentials.account.as_str()),
            ("passwd", credentials.password.as_str()),
            ("session", SESSION_NAME),
            ("format", "sid"),
        ];
        if let Some(otp) = &credentials.otp {
            form.push(("otp_code", otp.as_str()));
            if credentials.remember_device {
                form.push(("enable_device_token", "yes"));
                form.push(("device_name", credentials.device_name.as_str()));
            }
        }
        if let Some(token) = &credentials.device_token {
            form.push(("device_id", token.as_str()));
            form.push(("device_name", credentials.device_name.as_str()));
        }

        let data = self.post(&auth.path, "SYNO.API.Auth", &form).await?;
        let sid = data["sid"]
            .as_str()
            .filter(|sid| !sid.is_empty())
            .ok_or_else(|| Error::Unexpected("sign-in answer without a session".into()))?
            .to_owned();
        let device_token = data["did"]
            .as_str()
            .filter(|did| !did.is_empty())
            .map(str::to_owned)
            .or_else(|| credentials.device_token.clone());
        Ok(Session {
            sid,
            device_token,
            apis,
        })
    }

    /// Calls `api` with the highest version up to `preferred` that the NAS offers.
    async fn call(
        &self,
        session: &Session,
        api: &str,
        method: &str,
        preferred: u32,
        extra: &[(&str, &str)],
    ) -> Result<Value, Error> {
        let info = session.apis.get(api).ok_or_else(|| Error::Api {
            api: api.to_owned(),
            code: 102,
        })?;
        let version = preferred
            .min(info.max_version)
            .max(info.min_version)
            .to_string();
        let mut form = vec![
            ("api", api),
            ("version", version.as_str()),
            ("method", method),
            ("_sid", session.sid.as_str()),
        ];
        form.extend_from_slice(extra);
        self.post(&info.path, api, &form).await
    }

    /// Ends the session on the NAS. Errors are ignored: the session ends anyway.
    pub async fn logout(&self, session: &Session) {
        let _ = self
            .call(
                session,
                "SYNO.API.Auth",
                "logout",
                6,
                &[("session", SESSION_NAME)],
            )
            .await;
    }

    /// The server name and model DSM reports. Each part is an error on its own, so
    /// an account that may read only one of them still gets that one.
    pub async fn identity(
        &self,
        session: &Session,
    ) -> (Result<String, Error>, Result<String, Error>) {
        let named = |value: Result<Value, Error>, key: &str| {
            value.and_then(|value| {
                value[key]
                    .as_str()
                    .map(str::trim)
                    .filter(|text| !text.is_empty())
                    .map(str::to_owned)
                    .ok_or_else(|| Error::Unexpected(format!("no {key} in the answer")))
            })
        };
        let name = named(
            self.call(session, "SYNO.Core.Network", "get", 1, &[]).await,
            "server_name",
        );
        let model = named(
            self.call(session, "SYNO.Core.System", "info", 1, &[]).await,
            "model",
        );
        (name, model)
    }

    /// System, utilisation and storage in one picture.
    pub async fn overview(&self, session: &Session) -> Result<Overview, Error> {
        let system = self
            .call(session, "SYNO.Core.System", "info", 1, &[])
            .await?;
        let utilization = self
            .call(session, "SYNO.Core.System.Utilization", "get", 1, &[])
            .await?;
        let storage = self
            .call(session, "SYNO.Storage.CGI.Storage", "load_info", 1, &[])
            .await?;
        // Optional: accounts outside the administrators may not list the ports.
        let ethernet = self
            .call(session, "SYNO.Core.Network.Ethernet", "list", 2, &[])
            .await
            .unwrap_or(Value::Null);
        let overview = Overview::from_api(&system, &utilization, &storage, &ethernet);
        // Development builds show the port fields when no speed was recognised, to
        // check the field names against a real DSM. Field names only, no values.
        #[cfg(debug_assertions)]
        if overview
            .network_ports
            .iter()
            .all(|port| port.link_mbps.is_none())
            && let Some(port) = ethernet.as_array().and_then(|ports| ports.first())
        {
            let keys: Vec<&String> = port
                .as_object()
                .map(|o| o.keys().collect())
                .unwrap_or_default();
            eprintln!("[network] no speed recognised; port fields: {keys:?}");
        }
        Ok(overview)
    }
}

/// reqwest errors print the URL; ours carry no secrets, but keep messages short.
fn without_url(error: &reqwest::Error) -> String {
    let mut message = error.to_string();
    if let Some(url) = error.url() {
        message = message.replace(&format!(" ({url})"), "");
        message = message.replace(url.as_str(), "");
    }
    let mut source = std::error::Error::source(error);
    while let Some(cause) = source {
        message = format!("{message}: {cause}");
        source = cause.source();
    }
    message
}

#[cfg(test)]
mod tests;
