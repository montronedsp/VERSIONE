//! History listing for musician-facing Versions and checkpoints.

use std::path::Path;

use chrono::{DateTime, Utc};

use crate::error::Result;
use crate::project::{load_state, load_version_index, require_initialized, VersionRecord};
use crate::snapshot::SnapshotId;

/// One history row for CLI/UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryEntry {
    pub kind: HistoryEntryKind,
    pub display: Option<String>,
    pub name: Option<String>,
    pub snapshot_id: SnapshotId,
    pub created_utc: Option<DateTime<Utc>>,
    pub is_current: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryEntryKind {
    Version,
    Checkpoint,
}

/// List Versions newest-first, optionally including the current checkpoint tip.
pub fn list_history(root: &Path, include_checkpoint_tip: bool) -> Result<Vec<HistoryEntry>> {
    let paths = require_initialized(root)?;
    let state = load_state(&paths)?;
    let index = load_version_index(&paths)?;

    let mut entries = Vec::new();
    for record in index.versions.iter().rev() {
        entries.push(version_entry(record, state.current_version.as_ref()));
    }

    if include_checkpoint_tip {
        if let Some(checkpoint) = &state.current_checkpoint {
            let already = entries.iter().any(|e| &e.snapshot_id == checkpoint);
            if !already {
                entries.insert(
                    0,
                    HistoryEntry {
                        kind: HistoryEntryKind::Checkpoint,
                        display: None,
                        name: Some("latest checkpoint".into()),
                        snapshot_id: checkpoint.clone(),
                        created_utc: None,
                        is_current: true,
                    },
                );
            }
        }
    }

    Ok(entries)
}

fn version_entry(record: &VersionRecord, current: Option<&SnapshotId>) -> HistoryEntry {
    HistoryEntry {
        kind: HistoryEntryKind::Version,
        display: Some(record.display.clone()),
        name: Some(record.name.clone()),
        snapshot_id: record.snapshot_id.clone(),
        created_utc: Some(record.created_utc),
        is_current: current == Some(&record.snapshot_id),
    }
}

/// Resolve a musician-facing Version selector (`V01`, `V1`, name, or snapshot id).
pub fn resolve_version_selector(root: &Path, selector: &str) -> Result<VersionRecord> {
    let paths = require_initialized(root)?;
    let index = load_version_index(&paths)?;
    let sel = selector.trim();

    if let Some(record) = index
        .versions
        .iter()
        .find(|v| v.display.eq_ignore_ascii_case(sel))
    {
        return Ok(record.clone());
    }

    if let Some(num) = parse_version_number(sel) {
        if let Some(record) = index.versions.iter().find(|v| v.number == num) {
            return Ok(record.clone());
        }
    }

    if let Some(record) = index.versions.iter().find(|v| v.name == sel) {
        return Ok(record.clone());
    }

    if let Ok(id) = uuid::Uuid::parse_str(sel) {
        let snapshot = SnapshotId::from_uuid(id);
        if let Some(record) = index.versions.iter().find(|v| v.snapshot_id == snapshot) {
            return Ok(record.clone());
        }
    }

    Err(crate::error::Error::new(
        crate::error::ErrorKind::NotFound,
        format!("Version not found: {selector}"),
    ))
}

fn parse_version_number(sel: &str) -> Option<u32> {
    let s = sel.trim();
    let rest = s
        .strip_prefix('V')
        .or_else(|| s.strip_prefix('v'))
        .unwrap_or(s);
    rest.parse().ok()
}
