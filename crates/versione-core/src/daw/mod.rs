//! DAW inspection layer: identify projects and discover file-backed dependencies.
//!
//! Core VERSIONE remains DAW-agnostic for hashing, manifests, object storage,
//! snapshots, restore, and publication. Adapters supply identification and
//! dependency inspection only.

pub mod reaper;

use std::path::{Path, PathBuf};

use crate::error::Result;
use crate::project_root::{resolve_project_root, ProjectKind, ResolvedProjectRoot};

/// Summary of discovered media references for status / enable reporting.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DawReferenceSummary {
    pub total: usize,
    pub project_local: usize,
    pub external: usize,
    pub collected: usize,
    pub missing: usize,
    pub unsupported: usize,
    pub unresolved: usize,
    pub unsafe_path: usize,
}

/// High-level inspection outcome used by enable / status.
#[derive(Debug, Clone)]
pub struct DawInspection {
    pub kind: ProjectKind,
    pub project_file: Option<PathBuf>,
    pub references: DawReferenceSummary,
    pub notes: Vec<String>,
}

/// Inspect a resolved project for DAW-specific dependency information.
pub fn inspect_resolved(resolved: &ResolvedProjectRoot) -> Result<DawInspection> {
    match resolved.kind {
        ProjectKind::Reaper => {
            let rpp = resolved.primary_project_file.as_ref().ok_or_else(|| {
                crate::error::Error::new(
                    crate::error::ErrorKind::InvalidProject,
                    "REAPER project root is missing a primary .rpp file",
                )
            })?;
            let report = reaper::inspect_rpp(rpp)?;
            Ok(DawInspection {
                kind: ProjectKind::Reaper,
                project_file: Some(rpp.clone()),
                references: report.summary(),
                notes: report.notes,
            })
        }
        ProjectKind::Ableton => Ok(DawInspection {
            kind: ProjectKind::Ableton,
            project_file: resolved.primary_project_file.clone(),
            references: DawReferenceSummary::default(),
            notes: vec![
                "Ableton Set parsing is not used for reference discovery yet; supply --collect paths or keep assets project-local."
                    .into(),
            ],
        }),
        ProjectKind::Generic => Ok(DawInspection {
            kind: ProjectKind::Generic,
            project_file: None,
            references: DawReferenceSummary::default(),
            notes: Vec::new(),
        }),
    }
}

/// Convenience: resolve then inspect.
pub fn inspect_path(path: &Path) -> Result<(ResolvedProjectRoot, DawInspection)> {
    let resolved = resolve_project_root(path)?;
    let inspection = inspect_resolved(&resolved)?;
    Ok((resolved, inspection))
}
