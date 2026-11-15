//! Runs a script once as root through DSM's Task Scheduler: the only way to reach
//! root over the Web API. The calls are those of Control Panel › Task Scheduler in
//! DSM 7.4: confirm the administrator's password, create a disabled user-defined
//! script task owned by root, run it, wait for its exit code, delete it.

use std::time::Duration;

use serde_json::{Value, json};

use crate::{Client, Error, Session};

/// How long a script may take before the task is given up.
const WAIT: Duration = Duration::from_secs(60);
const POLL: Duration = Duration::from_millis(750);

/// DSM's answer to a wrong password in `SYNO.Core.User.PasswordConfirm`.
const WRONG_PASSWORD: i64 = 3121;

/// A form value as DSM's own interface sends it: JSON, so strings get quotes.
fn encoded(value: &Value) -> String {
    value.to_string()
}

/// A task that never triggers by itself: one date in the past, no repetition.
fn schedule() -> Value {
    json!({
        "date_type": 1,
        "date": "2000/1/1",
        "repeat_date": 0,
        "monthly_week": [],
        "hour": 0,
        "minute": 0,
        "repeat_hour": 0,
        "repeat_min": 0,
        "last_work_hour": 0,
        "repeat_min_store_config": [],
        "repeat_hour_store_config": [],
    })
}

fn extra(script: &str) -> Value {
    json!({
        "notify_enable": false,
        "notify_mail": "",
        "notify_if_error": false,
        "script": script,
    })
}

/// The task's ID from the answer to `create`.
pub(crate) fn created_id(data: &Value) -> Result<i64, Error> {
    data["id"]
        .as_i64()
        .ok_or_else(|| Error::Unexpected("the new task has no ID".into()))
}

/// The exit code of the task's run once it has finished; `None` while DSM has not
/// started it yet or it is still running. (DSM labels some exit codes
/// "by_signal"; the code itself is the script's.)
pub(crate) fn finished_exit_code(history: &Value) -> Option<i64> {
    let run = history.as_array()?.first()?;
    if run["exit_type"].as_str() == Some("running") {
        return None;
    }
    run["exit_code"].as_i64()
}

/// `[{"id": 7, "real_owner": "root"}]`: how `run` and `delete` name a task.
fn task_list(id: i64) -> String {
    json!([{ "id": id, "real_owner": "root" }]).to_string()
}

impl Client {
    /// Confirms the administrator's password; DSM wants this before it creates a
    /// task that runs as root. Returns the short-lived token for that call.
    async fn confirm_password(&self, session: &Session, password: &str) -> Result<String, Error> {
        let data = self
            .call(
                session,
                "SYNO.Core.User.PasswordConfirm",
                "auth",
                2,
                &[("password", password)],
            )
            .await
            .map_err(|error| match error {
                Error::Api { code, .. } if code == WRONG_PASSWORD => {
                    Error::Auth(crate::AuthError::WrongCredentials)
                }
                other => other,
            })?;
        data["SynoConfirmPWToken"]
            .as_str()
            .filter(|token| !token.is_empty())
            .map(str::to_owned)
            .ok_or_else(|| Error::Unexpected("no token for the password confirmation".into()))
    }

    /// Runs `script` once as root and returns its exit code. The task is deleted
    /// afterwards, also when running it failed. `password` is the signed-in
    /// administrator's, which DSM asks for again; `account` is that administrator.
    pub async fn run_as_root(
        &self,
        session: &Session,
        account: &str,
        password: &str,
        name: &str,
        script: &str,
    ) -> Result<i64, Error> {
        let token = self.confirm_password(session, password).await?;
        let created = self
            .call(
                session,
                "SYNO.Core.TaskScheduler.Root",
                "create",
                4,
                &[
                    ("name", &encoded(&json!(name))),
                    ("owner", &encoded(&json!("root"))),
                    ("real_owner", &encoded(&json!(account))),
                    ("enable", "false"),
                    ("type", &encoded(&json!("script"))),
                    ("schedule", &encoded(&schedule())),
                    ("extra", &encoded(&extra(script))),
                    ("SynoConfirmPWToken", &token),
                ],
            )
            .await?;
        let id = created_id(&created)?;
        let result = self.run_task(session, id).await;
        let deleted = self
            .call(
                session,
                "SYNO.Core.TaskScheduler",
                "delete",
                2,
                &[("tasks", &task_list(id))],
            )
            .await;
        // A root task left behind matters more than why the run failed.
        if let Err(error) = deleted {
            return Err(Error::Unexpected(format!(
                "the task {name:?} could not be deleted; remove it in Control Panel › Task Scheduler ({error})"
            )));
        }
        result
    }

    async fn run_task(&self, session: &Session, id: i64) -> Result<i64, Error> {
        self.call(
            session,
            "SYNO.Core.TaskScheduler",
            "run",
            2,
            &[("tasks", &task_list(id))],
        )
        .await?;
        let id = id.to_string();
        let started = std::time::Instant::now();
        loop {
            tokio::time::sleep(POLL).await;
            let history = self
                .call(
                    session,
                    "SYNO.Core.TaskScheduler",
                    "get_history_status_list",
                    1,
                    &[("id", &id), ("offset", "0"), ("limit", "1")],
                )
                .await?;
            if let Some(code) = finished_exit_code(&history) {
                return Ok(code);
            }
            if started.elapsed() > WAIT {
                return Err(Error::Unexpected("the task did not finish in time".into()));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_run_history() {
        // As DSM 7.4.1 answers `get_history_status_list`, newest run first.
        let running = json!([
            { "exit_code": 0, "exit_type": "running", "start_time": "2026-11-15 18:29:06",
              "stop_time": "", "timestamp": "1794763746" },
        ]);
        assert_eq!(finished_exit_code(&running), None);
        let done = json!([
            { "exit_code": 0, "exit_type": "normal", "start_time": "2026-11-15 18:29:06",
              "stop_time": "2026-11-15 18:29:09", "timestamp": "1794763746" },
            { "exit_code": 3, "exit_type": "by_signal", "start_time": "2026-11-15 18:25:50",
              "stop_time": "2026-11-15 18:25:50", "timestamp": "1794763550" },
        ]);
        assert_eq!(finished_exit_code(&done), Some(0));
        let failed = json!([
            { "exit_code": 10, "exit_type": "by_signal", "start_time": "2026-11-15 18:25:50",
              "stop_time": "2026-11-15 18:25:50", "timestamp": "1794763550" },
        ]);
        assert_eq!(finished_exit_code(&failed), Some(10));
        // Not started yet.
        assert_eq!(finished_exit_code(&json!([])), None);
        assert_eq!(finished_exit_code(&Value::Null), None);
    }

    #[test]
    fn writes_the_task_as_dsm_expects() {
        assert_eq!(created_id(&json!({ "id": 3 })), Ok(3));
        assert!(created_id(&json!({})).is_err());
        assert_eq!(task_list(3), r#"[{"id":3,"real_owner":"root"}]"#);
        assert_eq!(encoded(&json!("root")), r#""root""#);
        // DSM's task scheduler aborts on a schedule with fields missing.
        let schedule = schedule();
        for field in [
            "date_type",
            "date",
            "repeat_date",
            "monthly_week",
            "hour",
            "minute",
            "repeat_hour",
            "repeat_min",
            "last_work_hour",
            "repeat_min_store_config",
            "repeat_hour_store_config",
        ] {
            assert!(!schedule[field].is_null(), "{field}");
        }
        assert_eq!(extra("id")["script"], "id");
    }
}
