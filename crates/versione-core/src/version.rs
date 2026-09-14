//! Named Version creation and promotion from checkpoints.

use std::path::Path;
use std::time::Duration;

use chrono::Utc;

use crate::checkpoint::{create_checkpoint_locked, CheckpointOutcome};
use crate::error::{Error, ErrorKind, Result};
use crate::git::GitRepository;
use crate::lock::ProjectLock;
use crate::project::{
    load_state, load_version_index, require_initialized, save_state, save_version_index,
    VersionRecord,
};
use crate::snapshot::SnapshotId;

const LOCK_STALE: Duration = Duration::from_secs(30 * 60);
const MAX_NAME_LEN: usize = 200;

/// Result of keeping a named Version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionReport {
    pub number: u32,
    pub display: String,
    pub name: String,
    pub snapshot_id: SnapshotId,
    pub created_new_checkpoint: bool,
}

/// Snapshot current project if needed, then keep a named Version.
pub fn keep_version(root: &Path, name: &str) -> Result<VersionReport> {
    let name = validate_version_name(name)?;
    let paths = require_initialized(root)?;
    let _lock = ProjectLock::acquire(&paths, LOCK_STALE)?;

    let before = load_state(&paths)?;
    let checkpoint_outcome = create_checkpoint_locked(&paths)?;
    let (snapshot_id, created_new_checkpoint) = match checkpoint_outcome {
        CheckpointOutcome::Created(report) => (report.snapshot_id, true),
        CheckpointOutcome::Unchanged { snapshot_id, .. } => {
            // If there was no checkpoint tip before, still treat as needing the tip.
            let created = before.current_checkpoint.is_none();
            (snapshot_id, created)
        }
    };

    let mut state = load_state(&paths)?;
    let mut index = load_version_index(&paths)?;

    // Reuse existing Version record when promoting the same snapshot with same tip.
    if let Some(existing) = index
        .versions
        .iter()
        .find(|v| v.snapshot_id == snapshot_id && v.name == name)
    {
        return Ok(VersionReport {
            number: existing.number,
            display: existing.display.clone(),
            name: existing.name.clone(),
            snapshot_id,
            created_new_checkpoint,
        });
    }

    let number = state.next_version_number;
    let display = format!("V{number:02}");
    let record = VersionRecord {
        number,
        display: display.clone(),
        name: name.clone(),
        snapshot_id: snapshot_id.clone(),
        created_utc: Utc::now(),
    };
    index.versions.push(record);
    state.next_version_number = number.saturating_add(1);
    state.current_version = Some(snapshot_id.clone());
    state.current_checkpoint = Some(snapshot_id.clone());

    save_version_index(&paths, &index)?;
    save_state(&paths, &state)?;

    let git = GitRepository::new(&paths.root);
    let _ = git.commit_versione_metadata(&format!("Version {display}: {name}"))?;

    Ok(VersionReport {
        number,
        display,
        name,
        snapshot_id,
        created_new_checkpoint,
    })
}

pub fn validate_version_name(name: &str) -> Result<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(Error::new(
            ErrorKind::InvalidProject,
            "Version name must not be empty",
        ));
    }
    if trimmed.chars().count() > MAX_NAME_LEN {
        return Err(Error::new(
            ErrorKind::InvalidProject,
            format!("Version name exceeds maximum length of {MAX_NAME_LEN} characters"),
        ));
    }
    if trimmed.chars().any(|c| c.is_control()) {
        return Err(Error::new(
            ErrorKind::InvalidProject,
            "Version name must not contain control characters",
        ));
    }
    Ok(trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_blank_names() {
        assert!(validate_version_name("   ").is_err());
        assert!(validate_version_name("good name").is_ok());
    }
}
