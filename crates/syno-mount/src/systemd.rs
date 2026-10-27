//! One systemd user service per mount, plus the agent that remounts after suspend
//! and network changes. User services need no root and start at login.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::config::Mount;
use crate::paths::Paths;

pub const AGENT_UNIT: &str = "syno-mount-agent.service";

/// The program systemd starts. Inside an AppImage the running file lives in a
/// temporary mount, so the AppImage itself is used.
pub fn program() -> PathBuf {
    if let Some(appimage) = std::env::var_os("APPIMAGE") {
        return appimage.into();
    }
    std::env::current_exe().unwrap_or_else(|_| "/usr/bin/syno-mount".into())
}

/// systemd splits `Exec` lines at spaces; quote paths that contain any.
fn exec_path(path: &Path) -> String {
    let text = path.to_string_lossy();
    if text.contains([' ', '"', '\\']) {
        format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        text.into_owned()
    }
}

/// `%` starts a specifier in unit files.
fn escape_percent(text: &str) -> String {
    text.replace('%', "%%")
}

pub fn mount_unit(mount: &Mount, program: &Path, command_line: &str) -> String {
    let program = exec_path(program);
    let target = exec_path(&mount.local_path);
    format!(
        "# Written by Syno Mount. Changes are overwritten; use the app instead.\n\
         # The service runs:\n\
         #   {command}\n\
         [Unit]\n\
         Description=Syno Mount: {share} on {nas}\n\
         Documentation=https://github.com/SoulInfernoDE/syno-mount\n\
         \n\
         [Service]\n\
         Type=simple\n\
         ExecStartPre=/bin/mkdir -p {target}\n\
         ExecStart={program} run-mount {id}\n\
         ExecStopPost=-/usr/bin/fusermount3 -uz {target}\n\
         Restart=on-failure\n\
         RestartSec=10\n\
         \n\
         [Install]\n\
         WantedBy=default.target\n",
        command = escape_percent(command_line),
        share = escape_percent(&mount.share),
        nas = escape_percent(&mount.nas_name),
        id = mount.id,
    )
}

pub fn agent_unit(program: &Path) -> String {
    format!(
        "# Written by Syno Mount.\n\
         [Unit]\n\
         Description=Syno Mount agent: mounts again after suspend and network changes\n\
         Documentation=https://github.com/SoulInfernoDE/syno-mount\n\
         \n\
         [Service]\n\
         Type=simple\n\
         ExecStart={} agent\n\
         Restart=on-failure\n\
         RestartSec=5\n\
         \n\
         [Install]\n\
         WantedBy=default.target\n",
        exec_path(program)
    )
}

fn systemctl(arguments: &[&str]) -> io::Result<std::process::Output> {
    Command::new("systemctl")
        .arg("--user")
        .args(arguments)
        .output()
}

fn check(output: io::Result<std::process::Output>) -> io::Result<()> {
    let output = output?;
    if output.status.success() {
        Ok(())
    } else {
        Err(io::Error::other(
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        ))
    }
}

/// Writes a unit and makes systemd read it.
pub fn install(paths: &Paths, name: &str, text: &str) -> io::Result<()> {
    fs::create_dir_all(&paths.units)?;
    let path = paths.units.join(name);
    if fs::read_to_string(&path).ok().as_deref() != Some(text) {
        fs::write(&path, text)?;
        check(systemctl(&["daemon-reload"]))?;
    }
    Ok(())
}

pub fn uninstall(paths: &Paths, name: &str) -> io::Result<()> {
    let _ = systemctl(&["disable", "--now", name]);
    match fs::remove_file(paths.units.join(name)) {
        Ok(()) => check(systemctl(&["daemon-reload"])),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

pub fn start(name: &str) -> io::Result<()> {
    check(systemctl(&["start", name]))
}

pub fn stop(name: &str) -> io::Result<()> {
    check(systemctl(&["stop", name]))
}

pub fn restart(name: &str) -> io::Result<()> {
    check(systemctl(&["restart", name]))
}

/// Starts at login or not; leaves the current state alone.
pub fn set_enabled(name: &str, enabled: bool) -> io::Result<()> {
    check(systemctl(&[
        if enabled { "enable" } else { "disable" },
        name,
    ]))
}

/// "active", "activating", "failed", "inactive" …
pub fn state(name: &str) -> String {
    systemctl(&["is-active", name])
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .unwrap_or_else(|_| "unknown".into())
}

/// The last lines the service wrote, to explain a failure.
pub fn last_log(name: &str, lines: u32) -> String {
    Command::new("journalctl")
        .args([
            "--user",
            "-u",
            name,
            "-n",
            &lines.to_string(),
            "--no-pager",
            "-o",
            "cat",
        ])
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .unwrap_or_default()
}

/// Whether a systemd user instance runs here at all.
pub fn available() -> bool {
    systemctl(&["is-system-running"])
        .map(|output| {
            let state = String::from_utf8_lossy(&output.stdout);
            ["running", "degraded", "starting"].contains(&state.trim())
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::tests::sample;

    #[test]
    fn mount_units_run_the_wrapper_and_clean_up() {
        let dir = tempfile::tempdir().unwrap();
        let mut mount = sample(dir.path());
        mount.share = "100% Media".into();
        let unit = mount_unit(
            &mount,
            Path::new("/opt/Syno Mount/syno-mount"),
            "sshfs -f …",
        );
        assert!(
            unit.contains(
                "ExecStart=\"/opt/Syno Mount/syno-mount\" run-mount nas-local-5001-media\n"
            )
        );
        assert!(unit.contains("Description=Syno Mount: 100%% Media on nas\n"));
        assert!(unit.contains("#   sshfs -f …\n"));
        assert!(unit.contains("Restart=on-failure\n"));
        assert!(unit.contains("WantedBy=default.target\n"));
    }

    #[test]
    fn the_agent_unit_starts_the_agent() {
        let unit = agent_unit(Path::new("/usr/bin/syno-mount"));
        assert!(unit.contains("ExecStart=/usr/bin/syno-mount agent\n"));
    }
}
