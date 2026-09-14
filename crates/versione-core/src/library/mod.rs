//! Rebuildable local library index over VERSIONE projects.
//!
//! The library is an index, never the source of truth. Each project remains
//! self-describing under `.versione/`. If the index disappears, it can be rebuilt
//! from registered project roots.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{Error, ErrorKind, Result};
use crate::profile::{project_summary, ProjectSummary};
use crate::project::{read_toml, write_toml_atomic};
use crate::workflow::LifecycleState;

pub const LIBRARY_FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibraryEntry {
    pub root: PathBuf,
    pub project_id: String,
    pub registered_utc: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SmartCollection {
    pub name: String,
    #[serde(default)]
    pub query: FindQuery,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LibraryIndex {
    pub format_version: u32,
    #[serde(default)]
    pub projects: Vec<LibraryEntry>,
    #[serde(default)]
    pub collections: Vec<SmartCollection>,
}

impl LibraryIndex {
    pub fn new_v1() -> Self {
        Self {
            format_version: LIBRARY_FORMAT_VERSION,
            projects: Vec::new(),
            collections: Vec::new(),
        }
    }
}

/// Query filters for rediscovery / find. Domain API — not CLI formatting.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct FindQuery {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub genre: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subgenre: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bpm_min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bpm_max: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_note: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artist: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instrument: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifecycle: Option<LifecycleState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_stage: Option<crate::workflow::ReleaseStage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name_contains: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collaborator: Option<String>,
    /// Projects with no profile update for at least this many days.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inactive_for_days: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub has_unfinished_tasks: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub missing_preview: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_versions: Option<u32>,
}

fn default_library_path() -> Result<PathBuf> {
    if let Ok(custom) = std::env::var("VERSIONE_LIBRARY") {
        return Ok(PathBuf::from(custom));
    }
    let home = dirs_home().ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidProject,
            "cannot resolve home directory for library index; set VERSIONE_LIBRARY",
        )
    })?;
    Ok(home.join(".versione").join("library.toml"))
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

pub fn library_path() -> Result<PathBuf> {
    default_library_path()
}

pub fn load_library() -> Result<LibraryIndex> {
    let path = library_path()?;
    if !path.exists() {
        return Ok(LibraryIndex::new_v1());
    }
    let index: LibraryIndex = read_toml(&path)?;
    if index.format_version != LIBRARY_FORMAT_VERSION {
        return Err(Error::new(
            ErrorKind::UnsupportedFormat,
            format!(
                "unsupported library format_version {}",
                index.format_version
            ),
        )
        .with_path(path));
    }
    Ok(index)
}

pub fn save_library(index: &LibraryIndex) -> Result<()> {
    let path = library_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| Error::io("failed to create library directory", parent, e))?;
    }
    write_toml_atomic(&path, index)
}

pub fn register_project(root: &Path) -> Result<LibraryEntry> {
    let summary = project_summary(root)?;
    let mut index = load_library()?;
    index
        .projects
        .retain(|e| e.project_id != summary.project_id.to_string());
    let entry = LibraryEntry {
        root: root.canonicalize().unwrap_or_else(|_| root.to_path_buf()),
        project_id: summary.project_id.to_string(),
        registered_utc: Utc::now(),
    };
    index.projects.push(entry.clone());
    index.format_version = LIBRARY_FORMAT_VERSION;
    save_library(&index)?;
    Ok(entry)
}

/// Rebuild summaries from registered roots. Drops entries whose projects vanished.
pub fn rebuild_library() -> Result<Vec<ProjectSummary>> {
    let index = load_library()?;
    let mut kept = Vec::new();
    let mut summaries = Vec::new();
    for entry in index.projects {
        match project_summary(&entry.root) {
            Ok(summary) => {
                kept.push(LibraryEntry {
                    root: entry.root,
                    project_id: summary.project_id.to_string(),
                    registered_utc: entry.registered_utc,
                });
                summaries.push(summary);
            }
            Err(_) => continue,
        }
    }
    let mut next = LibraryIndex::new_v1();
    next.projects = kept;
    next.collections = index.collections;
    save_library(&next)?;
    Ok(summaries)
}

pub fn list_library_summaries() -> Result<Vec<ProjectSummary>> {
    let index = load_library()?;
    let mut out = Vec::new();
    for entry in &index.projects {
        if let Ok(summary) = project_summary(&entry.root) {
            out.push(summary);
        }
    }
    Ok(out)
}

fn norm(s: &str) -> String {
    s.trim().to_lowercase()
}

fn contains_ci(hay: &str, needle: &str) -> bool {
    norm(hay).contains(&norm(needle))
}

pub fn matches_query(summary: &ProjectSummary, query: &FindQuery) -> bool {
    if let Some(genre) = &query.genre {
        match &summary.genre {
            Some(g) if contains_ci(g, genre) => {}
            _ => return false,
        }
    }
    if let Some(sub) = &query.subgenre {
        match &summary.subgenre {
            Some(g) if contains_ci(g, sub) => {}
            _ => return false,
        }
    }
    if let Some(min) = query.bpm_min {
        match summary.bpm {
            Some(bpm) if bpm + f64::EPSILON >= min => {}
            _ => return false,
        }
    }
    if let Some(max) = query.bpm_max {
        match summary.bpm {
            Some(bpm) if bpm - f64::EPSILON <= max => {}
            _ => return false,
        }
    }
    if let Some(key) = &query.key {
        match &summary.key {
            Some(k) if contains_ci(k, key) => {}
            _ => return false,
        }
    }
    if let Some(scale) = &query.scale {
        match &summary.scale {
            Some(s) if contains_ci(s, scale) => {}
            _ => return false,
        }
    }
    if let Some(root_note) = &query.root_note {
        match &summary.root_note {
            Some(r) if contains_ci(r, root_note) => {}
            _ => return false,
        }
    }
    if let Some(artist) = &query.artist {
        let hit = summary
            .artist_aliases
            .iter()
            .any(|a| contains_ci(a, artist));
        if !hit {
            return false;
        }
    }
    if let Some(instrument) = &query.instrument {
        let hit = summary
            .instrument_names
            .iter()
            .any(|n| contains_ci(n, instrument));
        if !hit {
            return false;
        }
    }
    if let Some(tag) = &query.tag {
        let hit = summary.tags.iter().any(|t| contains_ci(t, tag));
        if !hit {
            return false;
        }
    }
    if let Some(lifecycle) = query.lifecycle {
        if summary.lifecycle != Some(lifecycle) {
            return false;
        }
    }
    if let Some(stage) = query.release_stage {
        if summary.release_stage != Some(stage) {
            return false;
        }
    }
    if let Some(name) = &query.name_contains {
        let title_hit = summary
            .title
            .as_ref()
            .map(|t| contains_ci(t, name))
            .unwrap_or(false);
        let alias_hit = summary.aliases.iter().any(|a| contains_ci(a, name));
        let path_hit = summary
            .root
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| contains_ci(n, name))
            .unwrap_or(false);
        if !(title_hit || alias_hit || path_hit) {
            return false;
        }
    }
    if let Some(days) = query.inactive_for_days {
        let cutoff = Utc::now() - Duration::days(days as i64);
        if summary.updated_utc > cutoff {
            return false;
        }
    }
    if let Some(true) = query.has_unfinished_tasks {
        if summary.unfinished_tasks == 0 {
            return false;
        }
    }
    if let Some(false) = query.has_unfinished_tasks {
        if summary.unfinished_tasks > 0 {
            return false;
        }
    }
    if let Some(true) = query.missing_preview {
        if summary.has_audio_preview {
            return false;
        }
    }
    if let Some(min_v) = query.min_versions {
        if summary.version_count < min_v {
            return false;
        }
    }
    let _ = &query.collaborator; // reserved; profile summary may gain collaborators later
    true
}

pub fn find_projects(query: &FindQuery) -> Result<Vec<ProjectSummary>> {
    let summaries = list_library_summaries()?;
    Ok(summaries
        .into_iter()
        .filter(|s| matches_query(s, query))
        .collect())
}

/// Evidence-based rediscovery hints (no judgmental language).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RediscoveryHint {
    pub message: String,
    pub matching: u32,
}

pub fn rediscovery_hints() -> Result<Vec<RediscoveryHint>> {
    let summaries = list_library_summaries()?;
    let mut hints = Vec::new();

    let pool_inactive = summaries
        .iter()
        .filter(|s| s.lifecycle == Some(LifecycleState::CreativePool))
        .filter(|s| {
            let cutoff = Utc::now() - Duration::days(180);
            s.updated_utc <= cutoff
        })
        .count();
    if pool_inactive > 0 {
        hints.push(RediscoveryHint {
            message: format!(
                "{pool_inactive} projects in Creative Pool have not been updated in 180 days."
            ),
            matching: pool_inactive as u32,
        });
    }

    let unfinished = summaries
        .iter()
        .filter(|s| s.unfinished_tasks > 0)
        .filter(|s| s.lifecycle != Some(LifecycleState::Released))
        .count();
    if unfinished > 0 {
        hints.push(RediscoveryHint {
            message: format!(
                "{unfinished} projects have unfinished tasks and are not marked Released."
            ),
            matching: unfinished as u32,
        });
    }

    let many_versions = summaries
        .iter()
        .filter(|s| s.version_count >= 3)
        .filter(|s| {
            !matches!(
                s.lifecycle,
                Some(LifecycleState::Released) | Some(LifecycleState::Archived)
            )
        })
        .count();
    if many_versions > 0 {
        hints.push(RediscoveryHint {
            message: format!(
                "{many_versions} projects have 3+ named versions and are not Released/Archived."
            ),
            matching: many_versions as u32,
        });
    }

    Ok(hints)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::update_profile;
    use crate::project::{init_project, InitOptions};
    use crate::workflow::LifecycleState;
    use tempfile::tempdir;

    #[test]
    fn find_by_genre_bpm_artist_and_state() {
        let dir = tempdir().unwrap();
        let lib = dir.path().join("library.toml");
        std::env::set_var("VERSIONE_LIBRARY", &lib);

        let p1 = dir.path().join("p1");
        fs::create_dir_all(&p1).unwrap();
        init_project(&p1, &InitOptions::default()).unwrap();
        update_profile(&p1, |p| {
            p.title = Some("Forgotten".into());
            p.artist_aliases = vec!["Crossing Avenue".into()];
            p.musical.genre = Some("Techno".into());
            p.musical.bpm = Some(135.0);
            p.musical.scale = Some("Phrygian".into());
            p.lifecycle = Some(LifecycleState::CreativePool);
        })
        .unwrap();
        register_project(&p1).unwrap();

        let hits = find_projects(&FindQuery {
            genre: Some("techno".into()),
            bpm_min: Some(130.0),
            bpm_max: Some(140.0),
            artist: Some("Crossing Avenue".into()),
            scale: Some("phrygian".into()),
            lifecycle: Some(LifecycleState::CreativePool),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(hits.len(), 1);

        std::env::remove_var("VERSIONE_LIBRARY");
    }

    #[test]
    fn library_survives_missing_index_rebuild() {
        let dir = tempdir().unwrap();
        let lib = dir.path().join("library.toml");
        std::env::set_var("VERSIONE_LIBRARY", &lib);
        let p1 = dir.path().join("p1");
        fs::create_dir_all(&p1).unwrap();
        init_project(&p1, &InitOptions::default()).unwrap();
        register_project(&p1).unwrap();
        let _ = fs::remove_file(&lib);
        // Project still usable without index.
        let summary = project_summary(&p1).unwrap();
        assert!(!summary.project_id.to_string().is_empty());
        register_project(&p1).unwrap();
        assert_eq!(list_library_summaries().unwrap().len(), 1);
        std::env::remove_var("VERSIONE_LIBRARY");
    }
}
