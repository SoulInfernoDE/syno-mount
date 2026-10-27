//! Errors as the user interface shows them: `key` picks the message
//! `error-<key>` from the Fluent catalog, `detail` adds technical text.

use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Failure {
    pub key: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl Failure {
    pub fn new(key: &'static str, detail: impl Into<Option<String>>) -> Self {
        Self {
            key,
            detail: detail.into(),
        }
    }

    pub fn io(error: impl ToString) -> Self {
        Self::new("io", error.to_string())
    }
}

impl From<syno_dsm::Error> for Failure {
    fn from(error: syno_dsm::Error) -> Self {
        use syno_dsm::Error;
        let detail = match &error {
            Error::Network(message) | Error::Unexpected(message) => Some(message.clone()),
            Error::Api { .. } | Error::PermissionDenied(_) => Some(error.to_string()),
            _ => None,
        };
        Self::new(error.key(), detail)
    }
}

impl From<syno_ssh::Error> for Failure {
    fn from(error: syno_ssh::Error) -> Self {
        use syno_ssh::Error;
        let detail = match &error {
            Error::Network(message) | Error::Sftp(message) | Error::Io(message) => {
                Some(message.clone())
            }
            Error::HostKeyUnknown { fingerprint } | Error::HostKeyChanged { fingerprint } => {
                Some(fingerprint.clone())
            }
            Error::AuthFailed => None,
        };
        Self::new(error.key(), detail)
    }
}

impl From<std::io::Error> for Failure {
    fn from(error: std::io::Error) -> Self {
        Self::io(error)
    }
}

impl From<tauri::Error> for Failure {
    fn from(error: tauri::Error) -> Self {
        Self::new("unexpected", error.to_string())
    }
}
