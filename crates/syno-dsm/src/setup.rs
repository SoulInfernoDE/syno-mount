//! First-time setup of a NAS whose DSM welcome wizard has not run yet.
//!
//! Until the wizard is done, DSM serves its setup session to whoever opens the web
//! interface. These calls do what DSM 7.4's own wizard does, in a safer order: the
//! new administrator account is created before the built-in `admin` account gets a
//! random password and is disabled, and the steps stop at the first error, so a
//! failed step never locks the owner out.

use serde::Serialize;
use serde_json::{Value, json};

use crate::{Client, Credentials, Error, Session};

/// Whether DSM's welcome wizard still has to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SetupState {
    Configured,
    NotConfigured,
}

/// DSM's rules for new passwords (`SYNO.Core.User.PasswordPolicy`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PasswordRules {
    pub min_length: Option<u32>,
    pub mixed_case: bool,
    pub numeric: bool,
    pub special: bool,
    pub exclude_username: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimeZone {
    /// DSM's zone ID, e.g. "Amsterdam".
    pub value: String,
    /// DSM's label, e.g. "(GMT+01:00) Amsterdam, Berlin, Rome, Stockholm, Vienna".
    pub display: String,
    /// Standard-time offset in seconds.
    pub offset: i64,
}

/// What the setup form needs to know before it is filled in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupInfo {
    pub device_name: String,
    pub password_rules: PasswordRules,
    /// Account names DSM does not allow, lower case.
    pub reserved_names: Vec<String>,
    pub time_zones: Vec<TimeZone>,
}

/// How DSM installs updates (`autoupdate_type` of the wizard's update step).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UpdatePolicy {
    /// Install important and security updates automatically (DSM's default).
    #[default]
    Important,
    /// Install every update automatically.
    All,
    /// Only notify.
    NotifyOnly,
}

#[derive(Debug, Clone)]
pub struct SetupRequest {
    pub device_name: String,
    pub account: String,
    pub password: String,
    pub time_zone: String,
    pub updates: UpdatePolicy,
    /// Shown in DSM's list of trusted devices.
    pub device_label: String,
}

/// The result of a finished setup: a session of the new account, and the optional
/// settings DSM refused (the setup itself still counts as done).
#[derive(Debug, Clone)]
pub struct SetupDone {
    pub session: Session,
    pub skipped: Vec<String>,
}

fn call(api: &str, method: &str, version: u32, params: Value) -> Value {
    let mut call = json!({ "api": api, "method": method, "version": version });
    if let (Some(call), Value::Object(params)) = (call.as_object_mut(), params) {
        call.extend(params);
    }
    call
}

/// A password DSM accepts under any policy: 24 characters from all four classes.
fn random_password() -> Result<String, Error> {
    const CLASSES: [&[u8]; 4] = [
        b"abcdefghijkmnopqrstuvwxyz",
        b"ABCDEFGHJKLMNPQRSTUVWXYZ",
        b"23456789",
        b"!#%+-=?@^_~",
    ];
    let mut bytes = [0u8; 48];
    ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut bytes)
        .map_err(|_| Error::Unexpected("no system randomness".into()))?;
    let mut password: Vec<u8> = (0..24)
        .map(|i| {
            let class = CLASSES[i % CLASSES.len()];
            class[usize::from(bytes[i]) % class.len()]
        })
        .collect();
    // Shuffle so the classes do not repeat in a fixed pattern.
    for i in (1..password.len()).rev() {
        let j = usize::from(bytes[24 + i]) % (i + 1);
        password.swap(i, j);
    }
    String::from_utf8(password).map_err(|error| Error::Unexpected(error.to_string()))
}

impl UpdatePolicy {
    fn requests(self) -> [Value; 2] {
        let (kind, packages) = match self {
            Self::Important => (
                "hotfix-security",
                json!({ "enable_autoupdate": true, "autoupdateimportant": true }),
            ),
            Self::All => (
                "hotfix",
                json!({ "enable_autoupdate": true, "autoupdateall": true }),
            ),
            Self::NotifyOnly => ("notify", json!({ "enable_autoupdate": false })),
        };
        [
            call(
                "SYNO.Core.Upgrade.Setting",
                "set",
                4,
                json!({ "autoupdate_type": kind }),
            ),
            call("SYNO.Core.Package.Setting", "set", 1, packages),
        ]
    }
}

impl Client {
    /// Asks DSM, without signing in, whether its welcome wizard still has to run.
    ///
    /// Only a NAS that is not set up hands out a session to whoever opens the web
    /// interface; a set-up NAS answers the permission check with "no session" (119).
    /// The client must be fresh, without a session of its own.
    pub async fn setup_state(&self) -> Result<SetupState, Error> {
        self.http
            .get(self.base.clone())
            .send()
            .await
            .map_err(|error| self.transport_error(&error))?;
        let url = self
            .base
            .join(
                "webapi/entry.cgi?api=SYNO.Core.QuickStart.Info&version=1&method=check_permission",
            )
            .map_err(|error| Error::Unexpected(error.to_string()))?;
        let answer: Value = self
            .http
            .get(url)
            .send()
            .await
            .map_err(|error| self.transport_error(&error))?
            .json()
            .await
            .map_err(|error| Error::Unexpected(error.to_string()))?;
        Ok(if answer["success"] == true {
            SetupState::NotConfigured
        } else {
            SetupState::Configured
        })
    }

    /// Several calls in one request (`SYNO.Entry.Request`), run one after another.
    /// Returns DSM's per-call results.
    async fn compound(
        &self,
        sid: Option<&str>,
        stop_when_error: bool,
        calls: &[Value],
    ) -> Result<Vec<Value>, Error> {
        let calls = serde_json::to_string(calls).map_err(|e| Error::Unexpected(e.to_string()))?;
        let stop = stop_when_error.to_string();
        let mut form = vec![
            ("api", "SYNO.Entry.Request"),
            ("method", "request"),
            ("version", "1"),
            ("stop_when_error", stop.as_str()),
            ("mode", "\"sequential\""),
            ("compound", calls.as_str()),
        ];
        if let Some(sid) = sid {
            form.push(("_sid", sid));
        }
        let data = self
            .post("entry.cgi/SYNO.Entry.Request", "SYNO.Entry.Request", &form)
            .await?;
        Ok(data["result"].as_array().cloned().unwrap_or_default())
    }

    /// Opens DSM's setup session and reads what the setup form needs.
    pub async fn begin_setup(&self) -> Result<SetupInfo, Error> {
        // Opening the web interface hands out the setup session as a cookie.
        self.http
            .get(self.base.clone())
            .send()
            .await
            .map_err(|error| self.transport_error(&error))?;
        let results = self
            .compound(
                None,
                false,
                &[
                    call(
                        "SYNO.Core.QuickStart.Info",
                        "check_permission",
                        1,
                        json!({}),
                    ),
                    call("SYNO.Core.User.PasswordPolicy", "get", 1, json!({})),
                    call("SYNO.Core.User.UsernamePolicy", "list", 1, json!({})),
                    call("SYNO.Core.Region.NTP", "listzone", 1, json!({})),
                    call("SYNO.Core.Network", "get", 1, json!({})),
                ],
            )
            .await?;
        let data = |api: &str| {
            results
                .iter()
                .find(|result| result["api"] == api && result["success"] == true)
                .map(|result| result["data"].clone())
        };
        if data("SYNO.Core.QuickStart.Info").is_none() {
            return Err(Error::PermissionDenied("SYNO.Core.QuickStart.Info".into()));
        }
        Ok(SetupInfo {
            device_name: data("SYNO.Core.Network")
                .and_then(|network| network["server_name"].as_str().map(str::to_owned))
                .unwrap_or_default(),
            password_rules: data("SYNO.Core.User.PasswordPolicy")
                .map(|policy| password_rules(&policy["strong_password"]))
                .unwrap_or_default(),
            reserved_names: data("SYNO.Core.User.UsernamePolicy")
                .map(|names| reserved_names(&names))
                .unwrap_or_default(),
            time_zones: data("SYNO.Core.Region.NTP")
                .map(|zones| time_zones(&zones["zonedata"]))
                .unwrap_or_default(),
        })
    }

    /// Runs the setup and signs in with the new account.
    pub async fn complete_setup(&self, request: &SetupRequest) -> Result<SetupDone, Error> {
        let builtin_password = random_password()?;
        let account = request.account.trim();
        let essential = [
            call(
                "SYNO.Core.Security.AutoBlock",
                "set",
                1,
                json!({ "attempts": 10, "enable": true, "enable_expire": false, "within_mins": 5 }),
            ),
            call(
                "SYNO.Core.Network",
                "set",
                1,
                json!({ "server_name": request.device_name.trim() }),
            ),
            call(
                "SYNO.Core.User",
                "create",
                1,
                json!({ "name": account, "password": request.password }),
            ),
            call(
                "SYNO.Core.Group.Member",
                "add",
                1,
                json!({ "group": "administrators", "name": [account] }),
            ),
            // Do not register the NAS with find.synology.com.
            call(
                "SYNO.Core.Service",
                "control",
                1,
                json!({ "service": [{ "service_id": "synoagentregisterd", "action": "stop" }] }),
            ),
            // Only now, with the new administrator in place: lock the built-in account.
            call(
                "SYNO.Core.User",
                "set",
                1,
                json!({ "name": "admin", "password": builtin_password }),
            ),
            call(
                "SYNO.Core.User",
                "set",
                1,
                json!({ "name": "admin", "expired": "now" }),
            ),
        ];
        let results = self.compound(None, true, &essential).await?;
        if let Some(failed) = first_failure(&results) {
            return Err(failed);
        }
        if results.len() < essential.len() {
            return Err(Error::Unexpected("DSM did not run every setup step".into()));
        }

        let session = self
            .login(&Credentials {
                account: account.to_owned(),
                password: request.password.clone(),
                device_name: request.device_label.clone(),
                ..Credentials::default()
            })
            .await?;

        let mut optional = request.updates.requests().to_vec();
        optional.extend([
            // The wizard asks about usage statistics; the privacy-friendly answer is no.
            call(
                "SYNO.Core.DataCollect",
                "set",
                1,
                json!({ "enable": false }),
            ),
            call(
                "SYNO.Core.Region.NTP",
                "setzone",
                1,
                json!({ "timezone": request.time_zone }),
            ),
            call(
                "SYNO.Core.QuickStart.Info",
                "hide_welcome",
                1,
                json!({ "welcome_upgrade_step": false }),
            ),
        ]);
        let results = self.compound(Some(&session.sid), false, &optional).await?;
        let skipped = results
            .iter()
            .filter(|result| result["success"] != true)
            .filter_map(|result| result["api"].as_str().map(str::to_owned))
            .collect();
        Ok(SetupDone { session, skipped })
    }
}

fn first_failure(results: &[Value]) -> Option<Error> {
    results
        .iter()
        .find(|result| result["success"] != true)
        .map(|result| {
            let api = result["api"].as_str().unwrap_or("SYNO.Entry.Request");
            let method = result["method"].as_str().unwrap_or_default();
            Error::Api {
                api: format!("{api} {method}").trim().to_owned(),
                code: result["error"]["code"].as_i64().unwrap_or(-1),
            }
        })
}

fn password_rules(policy: &Value) -> PasswordRules {
    let flag = |key: &str| policy[key].as_bool() == Some(true);
    PasswordRules {
        min_length: flag("min_length_enable")
            .then(|| policy["min_length"].as_u64())
            .flatten()
            .and_then(|length| u32::try_from(length).ok()),
        mixed_case: flag("mixed_case"),
        numeric: flag("included_numeric_char"),
        special: flag("included_special_char"),
        exclude_username: flag("exclude_username"),
    }
}

fn reserved_names(list: &Value) -> Vec<String> {
    let entries = list.as_array().or_else(|| list["users"].as_array());
    entries
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| entry.as_str().or_else(|| entry["name"].as_str()))
                .map(str::to_lowercase)
                .collect()
        })
        .unwrap_or_default()
}

fn time_zones(zones: &Value) -> Vec<TimeZone> {
    zones
        .as_array()
        .map(|zones| {
            zones
                .iter()
                .filter_map(|zone| {
                    Some(TimeZone {
                        value: zone["value"].as_str()?.to_owned(),
                        display: zone["display"].as_str()?.to_owned(),
                        offset: zone["offset"].as_i64().unwrap_or(0),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_passwords_use_every_class() {
        let password = random_password().unwrap();
        assert_eq!(password.len(), 24);
        assert!(password.chars().any(|c| c.is_ascii_lowercase()));
        assert!(password.chars().any(|c| c.is_ascii_uppercase()));
        assert!(password.chars().any(|c| c.is_ascii_digit()));
        assert!(password.chars().any(|c| !c.is_ascii_alphanumeric()));
        assert_ne!(password, random_password().unwrap());
    }

    #[test]
    fn password_rules_follow_the_policy() {
        let policy = json!({ "min_length_enable": true, "min_length": 8, "mixed_case": true,
                             "included_numeric_char": true, "included_special_char": false,
                             "exclude_username": true });
        assert_eq!(
            password_rules(&policy),
            PasswordRules {
                min_length: Some(8),
                mixed_case: true,
                numeric: true,
                special: false,
                exclude_username: true,
            }
        );
    }
}
