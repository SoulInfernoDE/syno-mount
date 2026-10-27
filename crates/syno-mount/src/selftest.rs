//! What a mounted folder can do, tried for real in a scratch folder: the user
//! learns before starting a program whether it will find what it needs.

use std::fs::{self, File, FileTimes, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Outcome {
    Works,
    /// Works with a limit the user should know.
    Limited,
    Fails,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Check {
    /// Stable ID; the interface picks its text by it, e.g. "selftest-chmod".
    pub id: &'static str,
    pub outcome: Outcome,
    /// Technical detail, e.g. the mode that came back.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

fn check(id: &'static str, outcome: Outcome, detail: impl Into<Option<String>>) -> Check {
    Check {
        id,
        outcome,
        detail: detail.into(),
    }
}

/// A scratch folder that is removed again, whatever happens.
struct Scratch(PathBuf);

impl Scratch {
    fn new(root: &Path) -> std::io::Result<Self> {
        let path = root.join(format!(".syno-mount-test-{}", std::process::id()));
        fs::create_dir(&path)?;
        Ok(Self(path))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// The user ID of this process, from `/proc` (no libc needed).
fn own_uid() -> Option<u32> {
    let status = fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|line| line.starts_with("Uid:"))?;
    line.split_whitespace().nth(1)?.parse().ok()
}

fn error(what: &str, error: std::io::Error) -> String {
    format!("{what}: {error}")
}

/// Runs every check in `root`, which must be a writable mounted folder.
pub fn run(root: &Path) -> Result<Vec<Check>, String> {
    let scratch = Scratch::new(root).map_err(|e| error("create a test folder", e))?;
    let dir = &scratch.0;
    let mut checks = Vec::new();

    // Write and read back.
    let file = dir.join("data");
    let payload: Vec<u8> = (0..(1 << 20)).map(|i| (i % 251) as u8).collect();
    let roundtrip = fs::write(&file, &payload).and_then(|()| fs::read(&file));
    checks.push(match roundtrip {
        Ok(read) if read == payload => check("write-read", Outcome::Works, None),
        Ok(_) => check("write-read", Outcome::Fails, "content differs".to_owned()),
        Err(e) => return Err(error("write a file", e)),
    });

    // Unix permissions: DSM ignores them in folders with Windows ACLs.
    let mode = fs::set_permissions(&file, fs::Permissions::from_mode(0o640))
        .and_then(|()| fs::metadata(&file))
        .map(|meta| meta.permissions().mode() & 0o777);
    checks.push(match mode {
        Ok(0o640) => check("chmod", Outcome::Works, None),
        Ok(other) => check("chmod", Outcome::Fails, format!("640 → {other:o}")),
        Err(e) => check("chmod", Outcome::Fails, e.to_string()),
    });

    // Owner: own files should show as one's own.
    let owner = fs::metadata(&file).map(|meta| meta.uid());
    checks.push(match (owner, own_uid()) {
        (Ok(uid), Some(own)) if uid == own => check("owner", Outcome::Works, None),
        (Ok(uid), _) => check("owner", Outcome::Limited, format!("uid {uid}")),
        (Err(e), _) => check("owner", Outcome::Fails, e.to_string()),
    });

    // Modification time, as build tools and sync programs use it.
    let when = SystemTime::UNIX_EPOCH + Duration::from_secs(1_577_836_800);
    let times = File::options()
        .write(true)
        .open(&file)
        .and_then(|handle| handle.set_times(FileTimes::new().set_modified(when)))
        .and_then(|()| fs::metadata(&file))
        .and_then(|meta| meta.modified());
    checks.push(match times {
        Ok(modified) if modified == when => check("mtime", Outcome::Works, None),
        Ok(_) => check("mtime", Outcome::Fails, "time not kept".to_owned()),
        Err(e) => check("mtime", Outcome::Fails, e.to_string()),
    });

    // Replace a file by renaming over it, as editors save.
    let other = dir.join("other");
    let renamed = fs::write(&other, b"old")
        .and_then(|()| fs::rename(&other, &file))
        .and_then(|()| fs::read(&file));
    checks.push(match renamed {
        Ok(content) if content == b"old" => check("rename-over", Outcome::Works, None),
        Ok(_) => check("rename-over", Outcome::Fails, "content differs".to_owned()),
        Err(e) => check("rename-over", Outcome::Fails, e.to_string()),
    });

    // Symbolic links: created and read back.
    let link = dir.join("link");
    let symlink = std::os::unix::fs::symlink("data", &link).and_then(|()| fs::read_link(&link));
    checks.push(match symlink {
        Ok(target) if target == Path::new("data") => check("symlink", Outcome::Works, None),
        Ok(target) => check(
            "symlink",
            Outcome::Limited,
            format!("→ {}", target.display()),
        ),
        Err(e) if link.symlink_metadata().is_ok() => {
            check("symlink", Outcome::Limited, error("read back", e))
        }
        Err(e) => check("symlink", Outcome::Fails, e.to_string()),
    });

    // Hard links.
    let hard = dir.join("hard");
    checks.push(match fs::hard_link(&file, &hard) {
        Ok(()) => check("hardlink", Outcome::Works, None),
        Err(e) => check("hardlink", Outcome::Fails, e.to_string()),
    });

    // Extended attributes: SFTP has none.
    checks.push(match xattr::set(&file, "user.syno-mount", b"1") {
        Ok(()) => check("xattr", Outcome::Works, None),
        Err(e) => check("xattr", Outcome::Fails, e.to_string()),
    });

    // Locks are granted, but only on this computer: SFTP does not pass them on.
    let locked = File::open(&file).and_then(|handle| {
        handle.try_lock().map_err(std::io::Error::other)?;
        handle.unlock()
    });
    checks.push(match locked {
        Ok(()) => check("lock", Outcome::Limited, None),
        Err(e) => check("lock", Outcome::Fails, e.to_string()),
    });

    // Files beyond 4 GiB, which FAT and some WebDAV servers refuse. Sparse, so fast.
    let big = dir.join("big");
    let size = 5u64 << 30;
    let large = File::create(&big).and_then(|mut handle| {
        handle.seek(SeekFrom::Start(size - 1))?;
        handle.write_all(&[1])?;
        handle.sync_all()?;
        fs::metadata(&big).map(|meta| meta.len())
    });
    let _ = fs::remove_file(&big);
    checks.push(match large {
        Ok(len) if len == size => check("large-file", Outcome::Works, None),
        Ok(len) => check("large-file", Outcome::Fails, format!("{len} bytes")),
        Err(e) => check("large-file", Outcome::Fails, e.to_string()),
    });

    Ok(checks)
}

/// Throughput of one large file and many small ones.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Speed {
    pub write_bytes_per_second: f64,
    pub read_bytes_per_second: f64,
    pub small_files_per_second: f64,
    pub megabytes: u32,
}

/// Writes and reads a file of `megabytes` MiB, then creates and removes 200 small
/// files.
pub fn speed(root: &Path, megabytes: u32) -> Result<Speed, String> {
    let scratch = Scratch::new(root).map_err(|e| error("create a test folder", e))?;
    let file = scratch.0.join("speed");
    let block: Vec<u8> = (0..(1 << 20)).map(|i| (i * 7 % 256) as u8).collect();
    let total = f64::from(megabytes) * f64::from(1u32 << 20);

    let started = Instant::now();
    {
        let mut handle = File::create(&file).map_err(|e| error("create", e))?;
        for _ in 0..megabytes {
            handle.write_all(&block).map_err(|e| error("write", e))?;
        }
        handle.sync_all().map_err(|e| error("sync", e))?;
    }
    let write = total / started.elapsed().as_secs_f64();

    let started = Instant::now();
    {
        let mut handle = OpenOptions::new()
            .read(true)
            .open(&file)
            .map_err(|e| error("open", e))?;
        let mut buffer = vec![0u8; 1 << 20];
        while handle.read(&mut buffer).map_err(|e| error("read", e))? > 0 {}
    }
    let read = total / started.elapsed().as_secs_f64();

    let count = 200u32;
    let started = Instant::now();
    for index in 0..count {
        fs::write(scratch.0.join(format!("small-{index}")), [0u8; 4096])
            .map_err(|e| error("small files", e))?;
    }
    for index in 0..count {
        fs::remove_file(scratch.0.join(format!("small-{index}")))
            .map_err(|e| error("small files", e))?;
    }
    let small = f64::from(count) / started.elapsed().as_secs_f64();

    Ok(Speed {
        write_bytes_per_second: write,
        read_bytes_per_second: read,
        small_files_per_second: small,
        megabytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_local_folder_passes_everything_but_locks() {
        let dir = tempfile::tempdir().unwrap();
        let checks = run(dir.path()).unwrap();
        for check in &checks {
            let expected = match check.id {
                "lock" => Outcome::Limited,
                // tmpfs has no user xattrs on older kernels.
                "xattr" => check.outcome,
                _ => Outcome::Works,
            };
            assert_eq!(check.outcome, expected, "{check:?}");
        }
        assert!(
            fs::read_dir(dir.path()).unwrap().next().is_none(),
            "scratch left over"
        );
    }

    #[test]
    fn measures_speed() {
        let dir = tempfile::tempdir().unwrap();
        let speed = speed(dir.path(), 2).unwrap();
        assert!(speed.write_bytes_per_second > 0.0 && speed.small_files_per_second > 0.0);
    }
}
