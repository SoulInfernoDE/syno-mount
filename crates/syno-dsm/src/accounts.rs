//! The administrator's view: which DSM accounts may use SFTP and which shared
//! folders they may open, and changing both. Every call needs an administrator.
//! Also preparing an account's home folder for key login, as root.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{Client, Error, Session};

/// DSM's application ID for SFTP in Control Panel › Application Privileges.
const SFTP_APP: &str = "SYNO.SFTP";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Access {
    Allowed,
    Denied,
}

/// Where an account's SFTP privilege comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SftpRule {
    /// The rule for everyone applies.
    Default,
    /// A rule for this account.
    Own(Access),
}

/// A local DSM account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub name: String,
    pub description: String,
    /// Member of "administrators".
    pub admin: bool,
    /// Disabled or expired; such accounts cannot sign in at all.
    pub disabled: bool,
    pub sftp: Access,
    pub sftp_rule: SftpRule,
}

/// A shared folder as the administrator sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminShare {
    pub name: String,
    /// "/volume1".
    pub volume: String,
    /// Windows ACL mode: DSM ignores chmod inside.
    pub acl: bool,
}

/// An account's own permission on a shared folder (Control Panel › Shared Folder ›
/// Permissions). Group permissions come on top.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Permission {
    /// Nothing set for the account itself.
    None,
    Deny,
    ReadOnly,
    ReadWrite,
    /// Set in the ACL editor; shown, but not changed here.
    Custom,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareAccess {
    pub account: String,
    pub permission: Permission,
    /// Administrators reach every folder through their group.
    pub admin: bool,
}

/// What an account gets on one shared folder: its own permission and what its
/// groups (and the folder's ACL) give it. DSM applies both: a denial wins, then the
/// wider of the two.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountShare {
    pub share: String,
    /// Windows ACL mode: DSM ignores chmod inside.
    pub acl: bool,
    pub own: Permission,
    pub groups: Permission,
}

/// How preparing key login for an account ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "outcome", content = "exitCode")]
pub enum KeyLoginPrepared {
    Done,
    /// The account has no home folder yet: DSM creates it at the first sign-in.
    NoHome,
    /// `.ssh` or `authorized_keys` is a link (or a hard-linked file): root does
    /// not follow it anywhere, and nothing in `.ssh` was changed.
    Unsafe,
    /// The script stopped with this exit code.
    Failed(i64),
}

/// Exit codes of [`key_login_script`].
const NO_HOME: i64 = 10;
/// `.ssh` or `authorized_keys` is a link or not a plain directory or file.
const UNSAFE: i64 = 15;

/// `word` for the shell: unchanged when it is plain, otherwise in single quotes.
fn shell_word(word: &str) -> String {
    let plain = !word.is_empty()
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '@'));
    if plain {
        word.to_owned()
    } else {
        format!("'{}'", word.replace('\'', r"'\''"))
    }
}

/// The commands that let OpenSSH take keys for `account`: DSM creates home
/// folders with Windows ACLs, which show as 777 when they let others write, and
/// sshd's `StrictModes` then ignores `authorized_keys`. The ACLs are removed from
/// the home, `.ssh` and `authorized_keys`, which get 755, 700 and 600. Keys already
/// in the file stay. The last line checks the result. Shown to the administrator
/// before it runs.
///
/// The account owns its home and could put links there that root would follow
/// (`.ssh -> /etc`). So root first takes the home (owner root, 755, no ACL), and
/// then `.ssh` (owner root, 700): from then on the account cannot add or swap
/// anything. Links and hard-linked files are refused; `chown -h` never follows a
/// link. The account gets both folders back at the end, also when a step fails.
pub fn key_login_script(account: &str) -> Result<String, Error> {
    if account.is_empty() || account.starts_with(['.', '-']) || account.contains(['/', '\n', '\0'])
    {
        return Err(Error::Unexpected(format!(
            "not an account name: {account:?}"
        )));
    }
    // The name goes into one assignment, quoted; everything after uses the
    // variable, so no later quoting (the trap) can break it apart.
    let name = shell_word(account);
    Ok(format!(
        "account={name}\n\
         cd /var/services/homes/\"$account\" || exit {NO_HOME}\n\
         trap 'chown -h \"$account\" .; [ -d .ssh ] && [ ! -L .ssh ] && chown -h \"$account\" .ssh' EXIT\n\
         synoacltool -del . >/dev/null 2>&1\n\
         chown root . || exit 12\n\
         chmod 755 . || exit 13\n\
         test \"$(stat -c %a .)\" = 755 || exit 13\n\
         [ -L .ssh ] && exit {UNSAFE}\n\
         [ -e .ssh ] && [ ! -d .ssh ] && exit {UNSAFE}\n\
         mkdir -p .ssh || exit 11\n\
         synoacltool -del .ssh >/dev/null 2>&1\n\
         chown root .ssh || exit 12\n\
         chmod 700 .ssh || exit 13\n\
         test \"$(stat -c %a .ssh)\" = 700 || exit 13\n\
         [ -L .ssh/authorized_keys ] && exit {UNSAFE}\n\
         [ -e .ssh/authorized_keys ] && [ ! -f .ssh/authorized_keys ] && exit {UNSAFE}\n\
         [ -e .ssh/authorized_keys ] && [ \"$(stat -c %h .ssh/authorized_keys)\" != 1 ] && exit {UNSAFE}\n\
         touch .ssh/authorized_keys || exit 11\n\
         synoacltool -del .ssh/authorized_keys >/dev/null 2>&1\n\
         chown -h \"$account\" .ssh/authorized_keys || exit 12\n\
         chmod 600 .ssh/authorized_keys || exit 13\n\
         chown -h \"$account\" .ssh . || exit 12\n\
         trap - EXIT\n\
         test \"$(stat -c %a . .ssh .ssh/authorized_keys | tr '\\n' ' ')\" = '755 700 600 ' || exit 14\n"
    ))
}

pub(crate) fn prepared(exit_code: i64) -> KeyLoginPrepared {
    match exit_code {
        0 => KeyLoginPrepared::Done,
        NO_HOME => KeyLoginPrepared::NoHome,
        UNSAFE => KeyLoginPrepared::Unsafe,
        other => KeyLoginPrepared::Failed(other),
    }
}

fn flag(item: &Value, key: &str) -> bool {
    item[key].as_bool().unwrap_or(false)
}

fn permission_of(item: &Value) -> Permission {
    if flag(item, "is_deny") {
        Permission::Deny
    } else if flag(item, "is_custom") {
        Permission::Custom
    } else if flag(item, "is_writable") {
        Permission::ReadWrite
    } else if flag(item, "is_readonly") {
        Permission::ReadOnly
    } else {
        Permission::None
    }
}

fn permission_json(account: &str, permission: Permission) -> Value {
    json!({
        "name": account,
        "is_deny": permission == Permission::Deny,
        "is_readonly": permission == Permission::ReadOnly,
        "is_writable": permission == Permission::ReadWrite,
        "is_custom": false,
    })
}

/// A rule allows when its allowed addresses include "0.0.0.0" (everywhere).
fn rule_access(rule: &Value) -> Access {
    let everywhere = |key: &str| {
        rule[key]
            .as_array()
            .is_some_and(|list| list.iter().any(|ip| ip == "0.0.0.0"))
    };
    if everywhere("deny_ip") || !everywhere("allow_ip") {
        Access::Denied
    } else {
        Access::Allowed
    }
}

pub(crate) fn accounts_from_api(users: &Value, admins: &Value, rules: &Value) -> Vec<Account> {
    let admin_names: Vec<&str> = admins["users"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|user| user["name"].as_str())
        .collect();
    let rules: Vec<&Value> = rules["rules"].as_array().into_iter().flatten().collect();
    let everyone = rules
        .iter()
        .find(|rule| rule["entity_type"] == "everyone")
        .map(|rule| rule_access(rule))
        .unwrap_or(Access::Allowed);
    let mut accounts: Vec<Account> = users["users"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|user| {
            let name = user["name"].as_str()?.to_owned();
            let own = rules
                .iter()
                .find(|rule| rule["entity_type"] == "user" && rule["entity_name"] == name.as_str())
                .map(|rule| rule_access(rule));
            Some(Account {
                admin: admin_names.contains(&name.as_str()),
                description: user["description"].as_str().unwrap_or_default().to_owned(),
                // "now" marks disabled accounts; a date marks one that will expire.
                disabled: user["expired"].as_str() == Some("now"),
                sftp: own.unwrap_or(everyone),
                sftp_rule: own.map_or(SftpRule::Default, SftpRule::Own),
                name,
            })
        })
        // DSM's built-in "guest" can never use SFTP.
        .filter(|account| account.name != "guest")
        .collect();
    accounts.sort_by_key(|account| account.name.to_lowercase());
    accounts
}

pub(crate) fn shares_from_api(data: &Value) -> Vec<AdminShare> {
    let mut shares: Vec<AdminShare> = data["shares"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|share| share["is_usb_share"].as_bool() != Some(true))
        .filter_map(|share| {
            Some(AdminShare {
                name: share["name"].as_str()?.to_owned(),
                volume: share["vol_path"].as_str().unwrap_or_default().to_owned(),
                acl: share["is_aclmode"].as_bool().unwrap_or(true),
            })
        })
        .collect();
    shares.sort_by_key(|share| share.name.to_lowercase());
    shares
}

/// DSM's `inherit`: "rw", "ro", "na" (no access) or "-" (nothing).
fn inherited(code: &str) -> Permission {
    match code {
        "rw" => Permission::ReadWrite,
        "ro" => Permission::ReadOnly,
        "na" => Permission::Deny,
        _ => Permission::None,
    }
}

pub(crate) fn account_shares_from_api(data: &Value) -> Vec<AccountShare> {
    let mut shares: Vec<AccountShare> = data["shares"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| {
            Some(AccountShare {
                share: item["name"].as_str()?.to_owned(),
                acl: item["is_aclmode"].as_bool().unwrap_or(true),
                own: permission_of(item),
                groups: inherited(item["inherit"].as_str().unwrap_or("-")),
            })
        })
        .collect();
    shares.sort_by_key(|share| share.share.to_lowercase());
    shares
}

pub(crate) fn access_from_api(data: &Value) -> Vec<ShareAccess> {
    data["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let account = item["name"].as_str()?.to_owned();
            (account != "guest").then(|| ShareAccess {
                permission: permission_of(item),
                admin: flag(item, "is_admin"),
                account,
            })
        })
        .collect()
}

impl Client {
    /// Local accounts with their administrator membership and SFTP privilege.
    pub async fn accounts(&self, session: &Session) -> Result<Vec<Account>, Error> {
        let users = self
            .call(
                session,
                "SYNO.Core.User",
                "list",
                1,
                &[
                    ("type", "local"),
                    ("offset", "0"),
                    ("limit", "-1"),
                    ("additional", r#"["description","expired"]"#),
                ],
            )
            .await?;
        let admins = self
            .call(
                session,
                "SYNO.Core.Group.Member",
                "list",
                1,
                &[("group", "administrators"), ("ingroup", "true")],
            )
            .await?;
        let rules = self
            .call(
                session,
                "SYNO.Core.AppPriv.Rule",
                "list",
                1,
                &[("app_id", SFTP_APP)],
            )
            .await?;
        Ok(accounts_from_api(&users, &admins, &rules))
    }

    /// Every shared folder, including those the administrator cannot open.
    pub async fn admin_shares(&self, session: &Session) -> Result<Vec<AdminShare>, Error> {
        let data = self
            .call(
                session,
                "SYNO.Core.Share",
                "list",
                1,
                &[("additional", r#"["vol_path","is_aclmode"]"#)],
            )
            .await?;
        Ok(shares_from_api(&data))
    }

    /// The accounts' own permissions on one shared folder.
    pub async fn share_access(
        &self,
        session: &Session,
        share: &str,
    ) -> Result<Vec<ShareAccess>, Error> {
        let data = self
            .call(
                session,
                "SYNO.Core.Share.Permission",
                "list",
                1,
                &[
                    ("name", share),
                    ("user_group_type", "local_user"),
                    ("offset", "0"),
                    ("limit", "-1"),
                    ("is_unite_permission", "false"),
                ],
            )
            .await?;
        Ok(access_from_api(&data))
    }

    /// One account's permissions on every shared folder, its own and its groups'.
    pub async fn account_access(
        &self,
        session: &Session,
        account: &str,
    ) -> Result<Vec<AccountShare>, Error> {
        let data = self
            .call(
                session,
                "SYNO.Core.Share.Permission",
                "list_by_user",
                1,
                &[
                    ("name", account),
                    ("user_group_type", "local_user"),
                    ("share_type", "all"),
                    ("additional", r#"["is_aclmode"]"#),
                ],
            )
            .await?;
        Ok(account_shares_from_api(&data))
    }

    /// Sets accounts' own permissions on one shared folder. `Custom` is left alone.
    pub async fn set_share_access(
        &self,
        session: &Session,
        share: &str,
        changes: &[(String, Permission)],
    ) -> Result<(), Error> {
        let permissions: Vec<Value> = changes
            .iter()
            .filter(|(_, permission)| *permission != Permission::Custom)
            .map(|(account, permission)| permission_json(account, *permission))
            .collect();
        if permissions.is_empty() {
            return Ok(());
        }
        let permissions = Value::Array(permissions).to_string();
        self.call(
            session,
            "SYNO.Core.Share.Permission",
            "set",
            1,
            &[
                ("name", share),
                ("user_group_type", "local_user"),
                ("permissions", &permissions),
            ],
        )
        .await
        .map(|_| ())
    }

    /// Runs [`key_login_script`] for `account` once as root. `admin` and
    /// `password` are the signed-in administrator's: DSM asks for the password
    /// again before anything runs as root.
    pub async fn prepare_key_login(
        &self,
        session: &Session,
        admin: &str,
        password: &str,
        account: &str,
    ) -> Result<KeyLoginPrepared, Error> {
        let script = key_login_script(account)?;
        let name = format!("Syno Mount key login {account}");
        let code = self
            .run_as_root(session, admin, password, &name, &script)
            .await?;
        Ok(prepared(code))
    }

    /// Gives an account its own SFTP rule, or removes it with `None` so the rule for
    /// everyone applies again.
    pub async fn set_sftp(
        &self,
        session: &Session,
        account: &str,
        access: Option<Access>,
    ) -> Result<(), Error> {
        let (method, rule) = match access {
            None => (
                "delete",
                json!({ "entity_type": "user", "entity_name": account, "app_id": SFTP_APP }),
            ),
            Some(access) => {
                let (allow, deny) = match access {
                    Access::Allowed => (json!(["0.0.0.0"]), json!([])),
                    Access::Denied => (json!([]), json!(["0.0.0.0"])),
                };
                (
                    "set",
                    json!({
                        "entity_type": "user",
                        "entity_name": account,
                        "app_id": SFTP_APP,
                        "allow_ip": allow,
                        "deny_ip": deny,
                    }),
                )
            }
        };
        let rules = json!([rule]).to_string();
        self.call(
            session,
            "SYNO.Core.AppPriv.Rule",
            method,
            1,
            &[("rules", &rules)],
        )
        .await
        .map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_accounts_with_admins_and_sftp_rules() {
        let users = json!({ "users": [
            { "name": "anna", "description": "", "expired": "normal" },
            { "name": "Ben", "description": "Kids", "expired": "normal" },
            { "name": "admin", "description": "System default user", "expired": "now" },
            { "name": "guest", "description": "Guest", "expired": "now" },
        ]});
        let admins = json!({ "users": [{ "name": "anna" }, { "name": "admin" }] });
        let rules = json!({ "rules": [
            { "entity_type": "everyone", "entity_name": "everyone",
              "allow_ip": ["0.0.0.0"], "deny_ip": [] },
            { "entity_type": "user", "entity_name": "Ben",
              "allow_ip": [], "deny_ip": ["0.0.0.0"] },
        ]});
        let accounts = accounts_from_api(&users, &admins, &rules);
        let names: Vec<_> = accounts.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["admin", "anna", "Ben"]);
        assert!(accounts[0].disabled && accounts[0].admin);
        assert_eq!(accounts[1].sftp, Access::Allowed);
        assert_eq!(accounts[1].sftp_rule, SftpRule::Default);
        assert_eq!(accounts[2].sftp, Access::Denied);
        assert_eq!(accounts[2].sftp_rule, SftpRule::Own(Access::Denied));
    }

    #[test]
    fn reads_permissions_and_writes_them_back() {
        let data = json!({ "items": [
            { "name": "anna", "is_admin": true, "is_custom": false, "is_deny": false,
              "is_readonly": false, "is_writable": false },
            { "name": "ben", "is_admin": false, "is_custom": false, "is_deny": false,
              "is_readonly": true, "is_writable": false },
            { "name": "guest", "is_admin": false, "is_custom": false, "is_deny": false,
              "is_readonly": false, "is_writable": false },
        ]});
        let access = access_from_api(&data);
        assert_eq!(access.len(), 2);
        assert_eq!(access[0].permission, Permission::None);
        assert!(access[0].admin);
        assert_eq!(access[1].permission, Permission::ReadOnly);
        let written = permission_json("ben", Permission::ReadWrite);
        assert_eq!(written["is_writable"], true);
        assert_eq!(written["is_readonly"], false);
        assert_eq!(permission_json("ben", Permission::Deny)["is_deny"], true);
    }

    #[test]
    fn reads_an_accounts_own_and_group_permissions() {
        // As DSM 7.2.2 answers `list_by_user`.
        let data = json!({ "shares": [
            { "name": "mounttest", "inherit": "-", "is_aclmode": true, "is_writable": true },
            { "name": "docker", "inherit": "na", "is_aclmode": true },
            { "name": "Archive", "inherit": "ro", "is_aclmode": false, "is_deny": true },
        ]});
        let shares = account_shares_from_api(&data);
        let names: Vec<&str> = shares.iter().map(|share| share.share.as_str()).collect();
        assert_eq!(names, ["Archive", "docker", "mounttest"]);
        assert_eq!(
            (shares[0].own, shares[0].groups, shares[0].acl),
            (Permission::Deny, Permission::ReadOnly, false)
        );
        assert_eq!(
            (shares[1].own, shares[1].groups),
            (Permission::None, Permission::Deny)
        );
        assert_eq!(
            (shares[2].own, shares[2].groups),
            (Permission::ReadWrite, Permission::None)
        );
    }

    #[test]
    fn writes_the_key_login_commands() {
        let script = key_login_script("anna").unwrap();
        let lines: Vec<&str> = script.lines().collect();
        assert_eq!(lines[0], "account=anna");
        assert_eq!(lines[1], r#"cd /var/services/homes/"$account" || exit 10"#);
        // The restore trap is set before root takes anything.
        assert!(lines[2].starts_with(r#"trap 'chown -h "$account" .;"#));
        // Root takes the home and .ssh before it looks at what is inside.
        let take_home = lines.iter().position(|l| *l == "chown root . || exit 12");
        let first_check = lines.iter().position(|l| *l == "[ -L .ssh ] && exit 15");
        assert!(take_home.unwrap() < first_check.unwrap());
        let take_ssh = lines
            .iter()
            .position(|l| *l == "chown root .ssh || exit 12");
        let key_check = lines
            .iter()
            .position(|l| *l == "[ -L .ssh/authorized_keys ] && exit 15");
        assert!(take_ssh.unwrap() < key_check.unwrap());
        // No chown that could follow a link.
        assert!(
            lines
                .iter()
                .filter(|l| l.starts_with("chown ") && !l.starts_with("chown root"))
                .all(|l| l.starts_with("chown -h "))
        );
        assert!(lines.contains(&r#"chown -h "$account" .ssh . || exit 12"#));
        assert!(lines.contains(&"chmod 600 .ssh/authorized_keys || exit 13"));
        assert_eq!(
            lines.last().unwrap(),
            &r#"test "$(stat -c %a . .ssh .ssh/authorized_keys | tr '\n' ' ')" = '755 700 600 ' || exit 14"#
        );
        // Names that the shell would read differently are quoted.
        let odd = key_login_script("o'neil smith").unwrap();
        assert!(odd.starts_with("account='o'\\''neil smith'\n"));
        assert_eq!(odd.matches("o'").count(), 1, "the name appears once");
        assert_eq!(shell_word("anna.b-c_d@x"), "anna.b-c_d@x");
        // Paths outside the homes are refused.
        for bad in ["", "..", "-rf", "a/b", "a\nb"] {
            assert!(key_login_script(bad).is_err(), "{bad:?}");
        }
        assert_eq!(prepared(0), KeyLoginPrepared::Done);
        assert_eq!(prepared(10), KeyLoginPrepared::NoHome);
        assert_eq!(prepared(15), KeyLoginPrepared::Unsafe);
        assert_eq!(prepared(14), KeyLoginPrepared::Failed(14));
        assert_eq!(
            serde_json::to_value(KeyLoginPrepared::Failed(14)).unwrap(),
            json!({ "outcome": "failed", "exitCode": 14 })
        );
        assert_eq!(
            serde_json::to_value(KeyLoginPrepared::Done).unwrap(),
            json!({ "outcome": "done" })
        );
    }

    #[test]
    fn reads_shares_without_usb() {
        let data = json!({ "shares": [
            { "name": "video", "vol_path": "/volume1", "is_aclmode": true },
            { "name": "usbshare1", "vol_path": "/volumeUSB1", "is_usb_share": true },
            { "name": "Backup", "vol_path": "/volume2", "is_aclmode": false },
        ]});
        let shares = shares_from_api(&data);
        assert_eq!(shares.len(), 2);
        assert_eq!(shares[0].name, "Backup");
        assert!(!shares[0].acl);
    }
}
