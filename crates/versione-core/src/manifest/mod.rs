//! Snapshot manifests describing project files and object references.
//!
//! Phase 1 policy: every project file's bytes are stored in the object store.
//! `FileClass` records classification for future Git-eligible policies, but Git
//! commits only VERSIONE metadata — not project file contents. See PROJECT_FORMAT.md.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::classify::FileClass;
use crate::error::{Error, ErrorKind, Result};
use crate::filesystem::ProjectRelativePath;
use crate::objects::{hash_reader, ObjectId};
use crate::snapshot::SnapshotId;
use std::io::Cursor;
use std::path::Path;

/// One file entry inside a snapshot manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestEntry {
    pub path: String,
    pub object_id: ObjectId,
    pub size: u64,
    pub class: FileClass,
}

impl ManifestEntry {
    pub fn new(
        path: ProjectRelativePath,
        object_id: ObjectId,
        size: u64,
        class: FileClass,
    ) -> Self {
        Self {
            path: path.as_str().to_string(),
            object_id,
            size,
            class,
        }
    }
}

/// Deterministic manifest for a Checkpoint or Version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotManifest {
    pub format_version: u32,
    pub snapshot_id: SnapshotId,
    /// Parent snapshot ids for history graph edges.
    pub parent_ids: Vec<SnapshotId>,
    pub created_utc: DateTime<Utc>,
    /// Digest of canonical entry payload; used to skip duplicate checkpoints.
    pub content_id: ObjectId,
    pub entries: Vec<ManifestEntry>,
}

impl SnapshotManifest {
    pub fn from_sorted_entries(
        snapshot_id: SnapshotId,
        parent_ids: Vec<SnapshotId>,
        mut entries: Vec<ManifestEntry>,
    ) -> Result<Self> {
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        let content_id = compute_content_id(&entries)?;
        Ok(Self {
            format_version: crate::project::FORMAT_VERSION,
            snapshot_id,
            parent_ids,
            created_utc: Utc::now(),
            content_id,
            entries,
        })
    }

    pub fn large_object_ids(&self) -> impl Iterator<Item = &ObjectId> {
        self.entries.iter().filter_map(|e| {
            if e.class == FileClass::LargeObject {
                Some(&e.object_id)
            } else {
                None
            }
        })
    }

    pub fn write_atomic(&self, path: &Path) -> Result<()> {
        let rendered = toml::to_string_pretty(self).map_err(|e| {
            Error::new(
                ErrorKind::Invariant,
                format!("failed to serialize manifest: {e}"),
            )
        })?;
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, rendered.as_bytes())
            .map_err(|e| Error::io("failed to write temporary manifest", &tmp, e))?;
        std::fs::rename(&tmp, path).map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            Error::io("failed to publish manifest", path, e)
        })?;
        Ok(())
    }

    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| Error::io("failed to read manifest", path, e))?;
        let manifest: Self = toml::from_str(&text).map_err(|e| {
            Error::new(
                ErrorKind::Corruption,
                format!("manifest is not valid TOML: {e}"),
            )
            .with_path(path)
        })?;
        if manifest.format_version != crate::project::FORMAT_VERSION {
            return Err(Error::new(
                ErrorKind::UnsupportedFormat,
                format!(
                    "unsupported manifest format_version {}",
                    manifest.format_version
                ),
            )
            .with_path(path));
        }
        let expected = compute_content_id(&manifest.entries)?;
        if expected != manifest.content_id {
            return Err(Error::new(
                ErrorKind::Corruption,
                "manifest content_id does not match entries",
            )
            .with_path(path));
        }
        Ok(manifest)
    }
}

fn compute_content_id(entries: &[ManifestEntry]) -> Result<ObjectId> {
    let mut payload = String::new();
    for entry in entries {
        let class = match entry.class {
            FileClass::GitEligible => "git",
            FileClass::LargeObject => "large",
        };
        payload.push_str(&format!(
            "{}\t{}\t{}\t{}\n",
            entry.path,
            entry.object_id.as_str(),
            entry.size,
            class
        ));
    }
    hash_reader(Cursor::new(payload.into_bytes()))
}
