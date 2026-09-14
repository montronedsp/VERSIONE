//! Explicit project lifecycle for Enable VERSIONE.
//!
//! ```text
//! UNMANAGED → COLLECTED → VERIFIED → VERSIONED → PUBLISHED
//! ```
//!
//! A snapshot is valid only after required objects are verified. Publication is
//! a separate remote-verification gate.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{Error, ErrorKind, Result};
use crate::project::{
    load_publication_index, load_state, read_toml, require_initialized, write_toml_atomic,
    ProjectPaths, FORMAT_VERSION,
};

/// On-disk schema for `.versione/lifecycle.toml`.
pub const LIFECYCLE_FORMAT_VERSION: u32 = 1;

/// Musician-facing lifecycle phase for an enabled project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecyclePhase {
    /// External assets collected into project-controlled storage (or confirmed none needed).
    Collected,
    /// Working tree / required objects passed integrity verification.
    Verified,
    /// At least one verified snapshot exists.
    Versioned,
    /// Latest snapshot objects verified remotely and Git metadata published.
    Published,
}

impl LifecyclePhase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Collected => "collected",
            Self::Verified => "verified",
            Self::Versioned => "versioned",
            Self::Published => "published",
        }
    }

    pub fn rank(self) -> u8 {
        match self {
            Self::Collected => 1,
            Self::Verified => 2,
            Self::Versioned => 3,
            Self::Published => 4,
        }
    }
}

impl std::fmt::Display for LifecyclePhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Derived or persisted view of enablement state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProjectLifecycle {
    Unmanaged,
    Phase(LifecyclePhase),
}

impl ProjectLifecycle {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unmanaged => "unmanaged",
            Self::Phase(p) => p.as_str(),
        }
    }
}

impl std::fmt::Display for ProjectLifecycle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Durable lifecycle record. Absence of this file on an initialized project is
/// treated as pre-lifecycle (legacy) and derived from snapshot/publication state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LifecycleDocument {
    pub format_version: u32,
    pub phase: LifecyclePhase,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collected_utc: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verified_utc: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub versioned_utc: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub published_utc: Option<DateTime<Utc>>,
    /// True when collection found no external assets to bring in.
    #[serde(default)]
    pub collection_noop: bool,
}

impl LifecycleDocument {
    pub fn new(phase: LifecyclePhase) -> Self {
        let now = Utc::now();
        let mut doc = Self {
            format_version: LIFECYCLE_FORMAT_VERSION,
            phase,
            collected_utc: None,
            verified_utc: None,
            versioned_utc: None,
            published_utc: None,
            collection_noop: false,
        };
        match phase {
            LifecyclePhase::Collected => doc.collected_utc = Some(now),
            LifecyclePhase::Verified => {
                doc.collected_utc = Some(now);
                doc.verified_utc = Some(now);
            }
            LifecyclePhase::Versioned => {
                doc.collected_utc = Some(now);
                doc.verified_utc = Some(now);
                doc.versioned_utc = Some(now);
            }
            LifecyclePhase::Published => {
                doc.collected_utc = Some(now);
                doc.verified_utc = Some(now);
                doc.versioned_utc = Some(now);
                doc.published_utc = Some(now);
            }
        }
        doc
    }
}

impl ProjectPaths {
    pub fn lifecycle_path(&self) -> std::path::PathBuf {
        self.versione_dir.join("lifecycle.toml")
    }
}

/// Whether `from → to` is a permitted transition (same phase or one adjacent step).
pub fn transition_allowed(from: ProjectLifecycle, to: LifecyclePhase) -> bool {
    match from {
        ProjectLifecycle::Unmanaged => to == LifecyclePhase::Collected,
        ProjectLifecycle::Phase(current) if current == to => true,
        ProjectLifecycle::Phase(LifecyclePhase::Collected) => to == LifecyclePhase::Verified,
        ProjectLifecycle::Phase(LifecyclePhase::Verified) => to == LifecyclePhase::Versioned,
        ProjectLifecycle::Phase(LifecyclePhase::Versioned) => to == LifecyclePhase::Published,
        ProjectLifecycle::Phase(LifecyclePhase::Published) => false,
    }
}

pub fn load_lifecycle_doc(paths: &ProjectPaths) -> Result<Option<LifecycleDocument>> {
    if !paths.lifecycle_path().exists() {
        return Ok(None);
    }
    let doc: LifecycleDocument = read_toml(&paths.lifecycle_path())?;
    if doc.format_version != LIFECYCLE_FORMAT_VERSION {
        return Err(Error::new(
            ErrorKind::UnsupportedFormat,
            format!(
                "unsupported lifecycle format_version {}",
                doc.format_version
            ),
        )
        .with_path(paths.lifecycle_path()));
    }
    let _ = FORMAT_VERSION;
    Ok(Some(doc))
}

pub fn save_lifecycle_doc(paths: &ProjectPaths, doc: &LifecycleDocument) -> Result<()> {
    write_toml_atomic(&paths.lifecycle_path(), doc)
}

/// Derive lifecycle from durable files when possible.
pub fn derive_lifecycle(paths: &ProjectPaths) -> Result<ProjectLifecycle> {
    if !paths.config_path().exists() {
        return Ok(ProjectLifecycle::Unmanaged);
    }
    let _ = require_initialized(&paths.root)?;
    if let Some(doc) = load_lifecycle_doc(paths)? {
        // Publication may advance beyond persisted phase when remotes verify.
        if (doc.phase == LifecyclePhase::Versioned || doc.phase == LifecyclePhase::Published)
            && current_snapshot_published(paths)?
        {
            return Ok(ProjectLifecycle::Phase(LifecyclePhase::Published));
        }
        return Ok(ProjectLifecycle::Phase(doc.phase));
    }

    // Legacy projects without lifecycle.toml.
    let state = load_state(paths)?;
    if state.current_checkpoint.is_some() {
        if current_snapshot_published(paths)? {
            return Ok(ProjectLifecycle::Phase(LifecyclePhase::Published));
        }
        return Ok(ProjectLifecycle::Phase(LifecyclePhase::Versioned));
    }
    Ok(ProjectLifecycle::Phase(LifecyclePhase::Collected))
}

fn current_snapshot_published(paths: &ProjectPaths) -> Result<bool> {
    let state = load_state(paths)?;
    let Some(id) = state.current_checkpoint.as_ref() else {
        return Ok(false);
    };
    let pub_index = load_publication_index(paths)?;
    Ok(pub_index
        .snapshots
        .iter()
        .any(|r| &r.snapshot_id == id && r.remote_verified && r.git_pushed))
}

pub fn advance_lifecycle(paths: &ProjectPaths, to: LifecyclePhase) -> Result<LifecycleDocument> {
    let current = derive_lifecycle(paths)?;
    if !transition_allowed(current, to) {
        return Err(Error::new(
            ErrorKind::Invariant,
            format!("invalid lifecycle transition: {current} → {to}"),
        ));
    }
    let mut doc = load_lifecycle_doc(paths)?.unwrap_or_else(|| LifecycleDocument::new(to));
    let now = Utc::now();
    doc.format_version = LIFECYCLE_FORMAT_VERSION;
    doc.phase = to;
    match to {
        LifecyclePhase::Collected => {
            doc.collected_utc = Some(doc.collected_utc.unwrap_or(now));
        }
        LifecyclePhase::Verified => {
            doc.collected_utc = Some(doc.collected_utc.unwrap_or(now));
            doc.verified_utc = Some(now);
        }
        LifecyclePhase::Versioned => {
            doc.collected_utc = Some(doc.collected_utc.unwrap_or(now));
            doc.verified_utc = Some(doc.verified_utc.unwrap_or(now));
            doc.versioned_utc = Some(now);
        }
        LifecyclePhase::Published => {
            doc.collected_utc = Some(doc.collected_utc.unwrap_or(now));
            doc.verified_utc = Some(doc.verified_utc.unwrap_or(now));
            doc.versioned_utc = Some(doc.versioned_utc.unwrap_or(now));
            doc.published_utc = Some(now);
        }
    }
    save_lifecycle_doc(paths, &doc)?;
    Ok(doc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forbids_skipping_to_published_from_collected() {
        assert!(!transition_allowed(
            ProjectLifecycle::Phase(LifecyclePhase::Collected),
            LifecyclePhase::Published
        ));
    }

    #[test]
    fn allows_versioned_after_verified() {
        assert!(transition_allowed(
            ProjectLifecycle::Phase(LifecyclePhase::Verified),
            LifecyclePhase::Versioned
        ));
    }

    #[test]
    fn forbids_unmanaged_to_versioned() {
        assert!(!transition_allowed(
            ProjectLifecycle::Unmanaged,
            LifecyclePhase::Versioned
        ));
    }

    #[test]
    fn forbids_collected_to_versioned_skip() {
        assert!(!transition_allowed(
            ProjectLifecycle::Phase(LifecyclePhase::Collected),
            LifecyclePhase::Versioned
        ));
    }
}
