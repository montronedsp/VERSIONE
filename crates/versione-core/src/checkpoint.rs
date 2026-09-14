//! Checkpoint creation: scan, ingest, deterministic manifest, publish tip.

use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::time::Duration;

use crate::classify::ClassifyPolicy;
use crate::error::{Error, ErrorKind, Result};
use crate::git::GitRepository;
use crate::ignore::IgnoreRules;
use crate::lock::ProjectLock;
use crate::manifest::{ManifestEntry, SnapshotManifest};
use crate::objects::{hash_file, ObjectId};
use crate::project::{load_state, require_initialized, save_state, ProjectPaths};
use crate::scan::scan_project;
use crate::snapshot::SnapshotId;
use crate::storage::{Availability, ObjectStore};

const LOCK_STALE: Duration = Duration::from_secs(30 * 60);

/// Result of attempting to create a checkpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckpointOutcome {
    Created(CheckpointReport),
    Unchanged {
        snapshot_id: SnapshotId,
        content_id: ObjectId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckpointReport {
    pub snapshot_id: SnapshotId,
    pub content_id: ObjectId,
    pub file_count: usize,
    pub large_object_count: usize,
    pub new_objects: usize,
    pub reused_objects: usize,
}

/// Create a checkpoint for the project at `root`.
pub fn create_checkpoint(root: &Path) -> Result<CheckpointOutcome> {
    let paths = require_initialized(root)?;
    let _lock = ProjectLock::acquire(&paths, LOCK_STALE)?;
    create_checkpoint_locked(&paths)
}

pub(crate) fn create_checkpoint_locked(paths: &ProjectPaths) -> Result<CheckpointOutcome> {
    // Verification before history: refuse a snapshot if required files cannot be read/hashed.
    let verification = crate::verify::verify_working_tree(&paths.root)?;
    verification.require_ok()?;

    let ignore = IgnoreRules::default();
    let policy = ClassifyPolicy::default();
    let scanned = scan_project(&paths.root, &ignore, &policy)?;
    let store = paths.open_local_store()?;

    let mut entries = Vec::with_capacity(scanned.len());
    let mut new_objects = 0usize;
    let mut reused_objects = 0usize;
    let mut large_object_count = 0usize;

    for file in &scanned {
        if file.class == crate::classify::FileClass::LargeObject {
            large_object_count += 1;
        }
        // Prefer staged self-contained bytes (e.g. rewritten REAPER .rpp) when present.
        let content_path = crate::daw::reaper::resolve_content_path(paths, file.relative.as_str())?;
        let size = std::fs::metadata(&content_path)
            .map_err(|e| Error::io("failed to stat content for checkpoint", &content_path, e))?
            .len();
        let object_id = hash_file(&content_path)?;
        if store.has(&object_id)? {
            match store.verify(&object_id)? {
                Availability::Available => reused_objects += 1,
                Availability::Corrupt => {
                    return Err(Error::new(
                        ErrorKind::Corruption,
                        format!("corrupt object {} for {}", object_id, file.relative),
                    ));
                }
                other => {
                    return Err(Error::new(
                        ErrorKind::Storage,
                        format!(
                            "object {} for {} has unexpected availability {other:?}",
                            object_id, file.relative
                        ),
                    ));
                }
            }
        } else {
            let file_handle = File::open(&content_path).map_err(|e| {
                Error::io(
                    "failed to open project file for object ingestion",
                    &content_path,
                    e,
                )
            })?;
            let mut reader = BufReader::with_capacity(1024 * 64, file_handle);
            store.put(&object_id, &mut reader)?;
            if store.verify(&object_id)? != Availability::Available {
                return Err(Error::new(
                    ErrorKind::Corruption,
                    format!("newly written object {object_id} failed verification"),
                ));
            }
            new_objects += 1;
        }

        entries.push(ManifestEntry::new(
            file.relative.clone(),
            object_id,
            size,
            file.class,
        ));
    }

    let state = load_state(paths)?;
    let parent_ids = state
        .current_checkpoint
        .clone()
        .into_iter()
        .collect::<Vec<_>>();

    let snapshot_id = SnapshotId::new();
    let manifest = SnapshotManifest::from_sorted_entries(snapshot_id.clone(), parent_ids, entries)?;

    if let Some(current) = &state.current_checkpoint {
        let current_path = paths.manifest_path(current);
        if current_path.exists() {
            let previous = SnapshotManifest::load(&current_path)?;
            if previous.content_id == manifest.content_id {
                return Ok(CheckpointOutcome::Unchanged {
                    snapshot_id: current.clone(),
                    content_id: previous.content_id,
                });
            }
        }
    }

    let manifest_path = paths.manifest_path(&manifest.snapshot_id);
    manifest.write_atomic(&manifest_path)?;

    // Re-load and verify before publishing tip.
    let verified = SnapshotManifest::load(&manifest_path)?;
    if verified.content_id != manifest.content_id {
        let _ = std::fs::remove_file(&manifest_path);
        return Err(Error::new(
            ErrorKind::Corruption,
            "published manifest failed verification",
        )
        .with_path(manifest_path));
    }

    let mut state = state;
    state.current_checkpoint = Some(manifest.snapshot_id.clone());
    save_state(paths, &state)?;

    let git = GitRepository::new(&paths.root);
    let _ = git.commit_versione_metadata(&format!(
        "Checkpoint {}",
        &manifest.snapshot_id.to_string()[..8]
    ))?;

    Ok(CheckpointOutcome::Created(CheckpointReport {
        snapshot_id: manifest.snapshot_id,
        content_id: manifest.content_id,
        file_count: scanned.len(),
        large_object_count,
        new_objects,
        reused_objects,
    }))
}
