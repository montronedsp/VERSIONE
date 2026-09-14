//! Publication state, remote object replication, and the Git publish gate.
//!
//! Invariant: never push Git metadata for a snapshot until every referenced
//! object has been uploaded and size-verified on the configured remote store.

use std::path::Path;
use std::time::Duration;

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::error::{Error, ErrorKind, Result};
use crate::git::GitRepository;
use crate::history::resolve_version_selector;
use crate::lock::ProjectLock;
use crate::manifest::SnapshotManifest;
use crate::project::{
    load_publication_index, load_state, load_storage_config, require_initialized,
    upsert_publication_record, ProjectPaths, PublicationRecord, RemoteStorageConfig,
};
use crate::snapshot::SnapshotId;
use crate::storage::{
    replicate_object, Availability, MemoryObjectStore, ObjectStore, S3Credentials, S3ObjectStore,
    S3StoreConfig, StorageLocationId, UreqS3Transport,
};

const LOCK_STALE: Duration = Duration::from_secs(30 * 60);

/// Musician-facing availability of a Version across layers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LayerStatus {
    Ok,
    Missing,
    Failed,
    NotAttempted,
    Unknown,
}

/// Separates local history, object-store replicas, and Git remotes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionAvailability {
    pub snapshot_id: SnapshotId,
    pub local_history: LayerStatus,
    pub object_store: LayerStatus,
    pub git_remote: LayerStatus,
    pub published: bool,
}

impl VersionAvailability {
    pub fn local_only(snapshot_id: SnapshotId) -> Self {
        Self {
            snapshot_id,
            local_history: LayerStatus::Ok,
            object_store: LayerStatus::NotAttempted,
            git_remote: LayerStatus::NotAttempted,
            published: false,
        }
    }

    pub fn recompute_published(&mut self) {
        self.published = matches!(
            (self.local_history, self.object_store, self.git_remote),
            (LayerStatus::Ok, LayerStatus::Ok, LayerStatus::Ok)
        );
    }
}

/// Ordered steps of a publish transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PublishStep {
    ResolveSnapshot,
    VerifyLocalObjects,
    UploadRemoteObjects,
    VerifyRemoteObjects,
    MarkRemotelyAvailable,
    PushGitRemote,
    MarkPublished,
}

/// Result of evaluating whether Git publication may proceed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishGate {
    pub allowed: bool,
    pub blocked_reason: Option<String>,
}

/// Enforce: remote object verification before Git remote push.
pub fn gate_git_push(objects_verified_remote: bool, manifest_written: bool) -> PublishGate {
    if !objects_verified_remote {
        return PublishGate {
            allowed: false,
            blocked_reason: Some(
                "refusing Git publish: required objects are not verified in remote object store"
                    .into(),
            ),
        };
    }
    if !manifest_written {
        return PublishGate {
            allowed: false,
            blocked_reason: Some("refusing Git publish: manifest not written".into()),
        };
    }
    PublishGate {
        allowed: true,
        blocked_reason: None,
    }
}

/// Planned object-store targets for a publish (no credentials).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublishStoragePlan {
    pub primary_local: StorageLocationId,
    pub remote: Option<StorageLocationId>,
}

/// Options controlling publish behavior.
#[derive(Clone, Default)]
pub struct PublishOptions {
    /// Optional Version selector (`V01`, name). Defaults to current Version/checkpoint.
    pub selector: Option<String>,
    /// When set, use this remote store instead of opening S3 from config (tests).
    pub remote_override: Option<std::sync::Arc<dyn ObjectStore + Send + Sync>>,
    /// Skip `git push` (still uploads/verifies remote objects). Useful in tests.
    pub skip_git_push: bool,
}

/// Report from a publish attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishReport {
    pub snapshot_id: SnapshotId,
    pub objects_uploaded: u64,
    pub objects_reused: u64,
    pub remote_verified: bool,
    pub git_pushed: bool,
    pub already_published: bool,
}

/// Publish the selected snapshot: remote objects first, then Git push.
pub fn publish_project(root: &Path, options: PublishOptions) -> Result<PublishReport> {
    let paths = require_initialized(root)?;
    let _lock = ProjectLock::acquire(&paths, LOCK_STALE)?;
    publish_locked(&paths, options)
}

fn publish_locked(paths: &ProjectPaths, options: PublishOptions) -> Result<PublishReport> {
    let snapshot_id = resolve_publish_snapshot(paths, options.selector.as_deref())?;
    let manifest = SnapshotManifest::load(&paths.manifest_path(&snapshot_id))?;
    let local = paths.open_local_store()?;

    // Local verification is mandatory before any remote work.
    for entry in &manifest.entries {
        match local.verify(&entry.object_id)? {
            Availability::Available => {}
            Availability::Missing => {
                return Err(Error::new(
                    ErrorKind::NotFound,
                    format!(
                        "local object {} missing for {}; cannot publish",
                        entry.object_id, entry.path
                    ),
                ));
            }
            Availability::Corrupt => {
                return Err(Error::new(
                    ErrorKind::Corruption,
                    format!(
                        "local object {} corrupt for {}; cannot publish",
                        entry.object_id, entry.path
                    ),
                ));
            }
            Availability::Unknown => {
                return Err(Error::new(
                    ErrorKind::Storage,
                    format!(
                        "local object {} availability unknown; cannot publish",
                        entry.object_id
                    ),
                ));
            }
        }
    }

    let remote: std::sync::Arc<dyn ObjectStore + Send + Sync> =
        if let Some(remote) = options.remote_override {
            remote
        } else {
            std::sync::Arc::from(open_configured_remote(paths)?)
        };

    let existing = load_publication_index(paths)?
        .snapshots
        .into_iter()
        .find(|r| r.snapshot_id == snapshot_id);

    let mut uploaded = 0u64;
    let mut reused = 0u64;

    for entry in &manifest.entries {
        match remote.metadata(&entry.object_id) {
            Ok(meta) if meta.size == entry.size => {
                reused += 1;
            }
            Ok(meta) => {
                return Err(Error::new(
                    ErrorKind::Corruption,
                    format!(
                        "remote object {} size {} does not match manifest size {}",
                        entry.object_id, meta.size, entry.size
                    ),
                ));
            }
            Err(err) if err.kind == ErrorKind::NotFound => {
                replicate_object(&local, remote.as_ref(), &entry.object_id, entry.size)?;
                uploaded += 1;
            }
            Err(err) => return Err(err),
        }
    }

    // Authoritative remote verification pass — cached publication metadata cannot skip this.
    for entry in &manifest.entries {
        let meta = remote.metadata(&entry.object_id).map_err(|err| {
            Error::new(
                ErrorKind::Publication,
                format!("remote verification failed for {}: {err}", entry.object_id),
            )
        })?;
        if meta.size != entry.size {
            return Err(Error::new(
                ErrorKind::Corruption,
                format!(
                    "remote object {} size {} != expected {}",
                    entry.object_id, meta.size, entry.size
                ),
            ));
        }
        if remote.verify(&entry.object_id)? != Availability::Available {
            return Err(Error::new(
                ErrorKind::Publication,
                format!(
                    "remote object {} not available after upload",
                    entry.object_id
                ),
            ));
        }
    }

    let now = Utc::now();
    upsert_publication_record(
        paths,
        PublicationRecord {
            snapshot_id: snapshot_id.clone(),
            remote_verified: true,
            remote_verified_utc: Some(now),
            git_pushed: existing.as_ref().map(|e| e.git_pushed).unwrap_or(false),
            git_pushed_utc: existing.as_ref().and_then(|e| e.git_pushed_utc),
            objects_uploaded: uploaded,
            objects_reused: reused,
        },
    )?;

    let gate = gate_git_push(true, true);
    if !gate.allowed {
        return Err(Error::new(
            ErrorKind::Publication,
            gate.blocked_reason
                .unwrap_or_else(|| "publication gate blocked Git push".into()),
        ));
    }

    let mut git_pushed = existing.as_ref().map(|e| e.git_pushed).unwrap_or(false);
    if !options.skip_git_push {
        let git = GitRepository::new(&paths.root);
        // Ensure latest publication metadata is committed locally before push.
        let _ = git.commit_versione_metadata(&format!(
            "Publish remote objects for {}",
            &snapshot_id.to_string()[..8]
        ))?;
        git.push_upstream()?;
        git_pushed = true;
        upsert_publication_record(
            paths,
            PublicationRecord {
                snapshot_id: snapshot_id.clone(),
                remote_verified: true,
                remote_verified_utc: Some(now),
                git_pushed: true,
                git_pushed_utc: Some(Utc::now()),
                objects_uploaded: uploaded,
                objects_reused: reused,
            },
        )?;
        let _ = git.commit_versione_metadata(&format!(
            "Mark published {}",
            &snapshot_id.to_string()[..8]
        ))?;
        // VERSIONED → PUBLISHED only after remote objects and Git push both succeeded.
        let _ =
            crate::lifecycle::advance_lifecycle(paths, crate::lifecycle::LifecyclePhase::Published);
    }

    let already_published = existing
        .as_ref()
        .map(|e| e.remote_verified && e.git_pushed && uploaded == 0)
        .unwrap_or(false);

    Ok(PublishReport {
        snapshot_id,
        objects_uploaded: uploaded,
        objects_reused: reused,
        remote_verified: true,
        git_pushed,
        already_published,
    })
}

fn resolve_publish_snapshot(paths: &ProjectPaths, selector: Option<&str>) -> Result<SnapshotId> {
    if let Some(sel) = selector {
        let version = resolve_version_selector(&paths.root, sel)?;
        return Ok(version.snapshot_id);
    }
    let state = load_state(paths)?;
    if let Some(id) = state.current_version.or(state.current_checkpoint) {
        return Ok(id);
    }
    Err(Error::new(
        ErrorKind::NotFound,
        "no Version or checkpoint available to publish",
    ))
}

fn open_configured_remote(paths: &ProjectPaths) -> Result<S3ObjectStore> {
    let storage = load_storage_config(paths)?;
    let Some(remote) = storage.remote else {
        return Err(Error::new(
            ErrorKind::InvalidProject,
            "no remote object store configured; run `versione storage set-remote`",
        ));
    };
    s3_store_from_config(&remote)
}

pub fn s3_store_from_config(remote: &RemoteStorageConfig) -> Result<S3ObjectStore> {
    if remote.kind != "s3" {
        return Err(Error::new(
            ErrorKind::UnsupportedFormat,
            format!("unsupported remote kind '{}'", remote.kind),
        ));
    }
    let config = S3StoreConfig {
        location_id: StorageLocationId::new(remote.location_id.clone()),
        bucket: remote.bucket.clone(),
        region: remote.region.clone(),
        endpoint: remote.endpoint.clone(),
        prefix: remote.prefix.clone(),
    };
    Ok(S3ObjectStore::new(
        config,
        S3Credentials::from_env()?,
        Box::new(UreqS3Transport),
    ))
}

/// Test helper: publish using an injected remote store without Git push.
pub fn publish_with_remote_for_test(
    root: &Path,
    remote: MemoryObjectStore,
    selector: Option<&str>,
) -> Result<PublishReport> {
    publish_project(
        root,
        PublishOptions {
            selector: selector.map(str::to_string),
            remote_override: Some(std::sync::Arc::new(remote)),
            skip_git_push: true,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn git_push_blocked_until_objects_verified() {
        let gate = gate_git_push(false, true);
        assert!(!gate.allowed);
        let ok = gate_git_push(true, true);
        assert!(ok.allowed);
    }

    #[test]
    fn published_requires_all_layers() {
        let mut avail = VersionAvailability::local_only(SnapshotId::new());
        assert!(!avail.published);
        avail.object_store = LayerStatus::Ok;
        avail.git_remote = LayerStatus::Ok;
        avail.recompute_published();
        assert!(avail.published);
    }
}
