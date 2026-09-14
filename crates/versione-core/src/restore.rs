//! Conservative restore-as-copy from a Version manifest.

use std::fs::{self, File};
use std::io::{BufWriter, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use crate::error::{Error, ErrorKind, Result};
use crate::filesystem::ProjectRelativePath;
use crate::history::resolve_version_selector;
use crate::lock::ProjectLock;
use crate::manifest::SnapshotManifest;
use crate::objects::ObjectId;
use crate::project::require_initialized;
use crate::snapshot::SnapshotId;
use crate::storage::{Availability, ObjectStore};

const LOCK_STALE: Duration = Duration::from_secs(30 * 60);

/// How restore should materialize content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestoreMode {
    /// Write into a new destination tree / renamed session copy.
    OpenAsCopy,
}

/// Validated restore request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestorePlan {
    pub snapshot_id: SnapshotId,
    pub destination: PathBuf,
    pub mode: RestoreMode,
}

impl RestorePlan {
    pub fn open_as_copy(snapshot_id: SnapshotId, destination: impl Into<PathBuf>) -> Result<Self> {
        let destination = destination.into();
        validate_destination(&destination)?;
        Ok(Self {
            snapshot_id,
            destination,
            mode: RestoreMode::OpenAsCopy,
        })
    }
}

/// Report from a successful restore-as-copy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreReport {
    pub snapshot_id: SnapshotId,
    pub version_display: Option<String>,
    pub version_name: Option<String>,
    pub destination: PathBuf,
    pub file_count: usize,
}

/// Restore a Version into an empty destination directory (open as copy).
pub fn restore_version_as_copy(
    root: &Path,
    selector: &str,
    destination: &Path,
) -> Result<RestoreReport> {
    let paths = require_initialized(root)?;
    let _lock = ProjectLock::acquire(&paths, LOCK_STALE)?;

    validate_destination(destination)?;
    if destination.exists() {
        return Err(Error::new(
            ErrorKind::AlreadyExists,
            "restore destination already exists; choose an empty path",
        )
        .with_path(destination));
    }

    let version = resolve_version_selector(root, selector)?;
    let plan = RestorePlan::open_as_copy(version.snapshot_id.clone(), destination)?;
    let manifest = SnapshotManifest::load(&paths.manifest_path(&plan.snapshot_id))?;
    let store = paths.open_local_store()?;

    for entry in &manifest.entries {
        match store.verify(&entry.object_id)? {
            Availability::Available => {}
            Availability::Missing => {
                return Err(Error::new(
                    ErrorKind::NotFound,
                    format!(
                        "missing object {} required by {}",
                        entry.object_id, entry.path
                    ),
                ));
            }
            Availability::Corrupt => {
                return Err(Error::new(
                    ErrorKind::Corruption,
                    format!(
                        "corrupt object {} required by {}",
                        entry.object_id, entry.path
                    ),
                ));
            }
            Availability::Unknown => {
                return Err(Error::new(
                    ErrorKind::Storage,
                    format!("object {} availability unknown", entry.object_id),
                ));
            }
        }
    }

    let staging_parent = destination
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    fs::create_dir_all(&staging_parent).map_err(|e| {
        Error::io(
            "failed to create restore destination parent",
            &staging_parent,
            e,
        )
    })?;

    let staging = staging_parent.join(format!(
        ".versione-restore-{}-{}",
        &plan.snapshot_id.to_string()[..8],
        std::process::id()
    ));
    if staging.exists() {
        fs::remove_dir_all(&staging)
            .map_err(|e| Error::io("failed to clear leftover restore staging", &staging, e))?;
    }
    fs::create_dir_all(&staging)
        .map_err(|e| Error::io("failed to create restore staging directory", &staging, e))?;

    let restore_result = (|| -> Result<usize> {
        let mut count = 0usize;
        for entry in &manifest.entries {
            let rel = ProjectRelativePath::new(&entry.path)?;
            let dest_file = join_under(&staging, &rel)?;
            if let Some(parent) = dest_file.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| Error::io("failed to create restore subdirectory", parent, e))?;
            }

            let mut reader = store.get(&entry.object_id)?;
            let file = File::create(&dest_file)
                .map_err(|e| Error::io("failed to create restored file", &dest_file, e))?;
            let mut writer = BufWriter::with_capacity(1024 * 64, file);
            let mut buffer = [0_u8; 1024 * 64];
            let mut hasher = blake3::Hasher::new();
            let mut size = 0_u64;
            loop {
                let read = reader.read(&mut buffer).map_err(|e| {
                    Error::new(ErrorKind::Io, "failed while reading object during restore")
                        .with_source(e)
                })?;
                if read == 0 {
                    break;
                }
                writer
                    .write_all(&buffer[..read])
                    .map_err(|e| Error::io("failed while writing restored file", &dest_file, e))?;
                hasher.update(&buffer[..read]);
                size += read as u64;
            }
            writer
                .flush()
                .map_err(|e| Error::io("failed to flush restored file", &dest_file, e))?;

            let actual = ObjectId::parse(&hasher.finalize().to_hex())?;
            if actual != entry.object_id {
                return Err(Error::new(
                    ErrorKind::Corruption,
                    format!(
                        "restored bytes for {} hashed to {actual}, expected {}",
                        entry.path, entry.object_id
                    ),
                )
                .with_path(dest_file));
            }
            if size != entry.size {
                return Err(Error::new(
                    ErrorKind::Corruption,
                    format!(
                        "restored size for {} was {size}, expected {}",
                        entry.path, entry.size
                    ),
                )
                .with_path(dest_file));
            }
            count += 1;
        }
        Ok(count)
    })();

    let file_count = match restore_result {
        Ok(count) => count,
        Err(err) => {
            let _ = fs::remove_dir_all(&staging);
            return Err(err);
        }
    };

    fs::rename(&staging, destination).map_err(|e| {
        let _ = fs::remove_dir_all(&staging);
        Error::io(
            "failed to publish restored project directory",
            destination,
            e,
        )
    })?;

    Ok(RestoreReport {
        snapshot_id: plan.snapshot_id,
        version_display: Some(version.display),
        version_name: Some(version.name),
        destination: destination.to_path_buf(),
        file_count,
    })
}

fn validate_destination(destination: &Path) -> Result<()> {
    if destination.as_os_str().is_empty() {
        return Err(Error::new(
            ErrorKind::InvalidProject,
            "restore destination must not be empty",
        ));
    }
    if let Some(name) = destination.file_name() {
        if name == ".versione" {
            return Err(Error::new(
                ErrorKind::PathEscape,
                "refusing to restore directly into a .versione directory",
            )
            .with_path(destination));
        }
    }
    Ok(())
}

fn join_under(root: &Path, relative: &ProjectRelativePath) -> Result<PathBuf> {
    let mut acc = root.to_path_buf();
    for component in Path::new(relative.as_str()).components() {
        match component {
            Component::Normal(part) => acc.push(part),
            Component::CurDir => {}
            _ => {
                return Err(Error::new(
                    ErrorKind::PathEscape,
                    "invalid component while joining restore path",
                )
                .with_path(relative.as_str()));
            }
        }
    }
    Ok(acc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_versione_destination() {
        let id = SnapshotId::new();
        let err = RestorePlan::open_as_copy(id, PathBuf::from(".versione")).unwrap_err();
        assert_eq!(err.kind, ErrorKind::PathEscape);
    }
}
