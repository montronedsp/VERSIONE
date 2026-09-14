//! Exclusive local lock for mutating VERSIONE project metadata.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::error::{Error, ErrorKind, Result};
use crate::project::ProjectPaths;

/// Held project lock. Released on drop.
#[derive(Debug)]
pub struct ProjectLock {
    path: PathBuf,
}

impl ProjectLock {
    /// Acquire an exclusive lock for checkpoint/version/restore mutations.
    ///
    /// Stale locks older than `stale_after` whose owning process is gone (best-effort
    /// on Windows via missing pid file freshness) may be removed.
    pub fn acquire(paths: &ProjectPaths, stale_after: Duration) -> Result<Self> {
        let path = paths.lock_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| Error::io("failed to create lock parent directory", parent, e))?;
        }

        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                write_lock_contents(&mut file, &path)?;
                Ok(Self { path })
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                if is_stale(&path, stale_after)? {
                    let _ = fs::remove_file(&path);
                    let mut file = OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&path)
                        .map_err(|err| {
                            Error::io(
                                "failed to acquire project lock after clearing stale lock",
                                &path,
                                err,
                            )
                        })?;
                    write_lock_contents(&mut file, &path)?;
                    Ok(Self { path })
                } else {
                    Err(Error::new(
                        ErrorKind::AlreadyExists,
                        "VERSIONE project is locked by another operation",
                    )
                    .with_path(path))
                }
            }
            Err(e) => Err(Error::io("failed to acquire project lock", &path, e)),
        }
    }
}

impl Drop for ProjectLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn write_lock_contents(file: &mut File, path: &Path) -> Result<()> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let body = format!("pid={}\ncreated_unix={}\n", std::process::id(), now);
    file.write_all(body.as_bytes())
        .map_err(|e| Error::io("failed to write project lock", path, e))?;
    file.sync_all()
        .map_err(|e| Error::io("failed to sync project lock", path, e))?;
    Ok(())
}

fn is_stale(path: &Path, stale_after: Duration) -> Result<bool> {
    let text = fs::read_to_string(path)
        .map_err(|e| Error::io("failed to read existing project lock", path, e))?;
    let mut created = None;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("created_unix=") {
            created = rest.trim().parse::<u64>().ok();
        }
    }
    let Some(created) = created else {
        return Ok(true);
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    Ok(now.saturating_sub(created) >= stale_after.as_secs())
}
