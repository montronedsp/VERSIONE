//! Library service APIs for Studio cards and project discovery.

use std::collections::{HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{Error, ErrorKind, Result};
use crate::library::{load_library, register_project, save_library, LibraryEntry};
use crate::media::{load_or_default_media, MediaId, MediaKind};
use crate::profile::project_summary;
use crate::project::{
    load_config, load_publication_index, load_storage_config, ProjectId, ProjectPaths,
};
use crate::workflow::LifecycleState;

/// UI-ready summary card for a project in the Studio library.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectCard {
    pub project_id: ProjectId,
    pub root_path: PathBuf,
    pub title: String,
    pub artists: Vec<String>,
    pub genre: Option<String>,
    pub bpm: Option<f64>,
    pub root: Option<String>,
    pub scale: Option<String>,
    pub lifecycle: Option<LifecycleState>,
    pub version_count: u32,
    pub channels: Option<u32>,
    pub unfinished_tasks: u32,
    pub last_activity: DateTime<Utc>,
    pub has_audio_preview: bool,
    pub has_arrangement_image: bool,
    pub audio_preview_media_id: Option<MediaId>,
    pub arrangement_image_media_id: Option<MediaId>,
    pub backup_state: BackupState,
}

/// Coarse backup/readiness state for library cards.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BackupState {
    LocalOnly,
    RemoteConfigured,
    RemoteVerified,
    Published,
    Unknown,
}

/// Bounded recursive discovery options.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveryOptions {
    pub max_depth: usize,
    pub include_root: bool,
}

impl Default for DiscoveryOptions {
    fn default() -> Self {
        Self {
            max_depth: 6,
            include_root: true,
        }
    }
}

/// A VERSIONE project found on disk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveredProject {
    pub root_path: PathBuf,
    pub project_id: ProjectId,
    pub title: Option<String>,
}

/// Result of updating a library entry after a project moved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MoveResolution {
    pub project_id: ProjectId,
    pub previous_root: Option<PathBuf>,
    pub new_root: PathBuf,
}

/// Non-destructive ProjectId conflict reports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProjectIdConflict {
    NewRootIsDifferentProject {
        expected: ProjectId,
        actual: ProjectId,
        root: PathBuf,
    },
    DuplicateRegisteredRoot {
        project_id: ProjectId,
        existing_root: PathBuf,
        new_root: PathBuf,
    },
}

/// List registered projects as Studio cards. Broken index entries are skipped.
pub fn list_library_cards() -> Result<Vec<ProjectCard>> {
    let mut cards = Vec::new();
    for summary in crate::library::list_library_summaries()? {
        if let Ok(card) = card_from_root(&summary.root) {
            cards.push(card);
        }
    }
    cards.sort_by(|a, b| {
        b.last_activity
            .cmp(&a.last_activity)
            .then_with(|| a.title.cmp(&b.title))
            .then_with(|| a.project_id.to_string().cmp(&b.project_id.to_string()))
    });
    Ok(cards)
}

pub(crate) fn card_from_root(root: &Path) -> Result<ProjectCard> {
    let summary = project_summary(root)?;
    let paths = ProjectPaths::from_root(root);
    let media = load_or_default_media(&paths)?;
    let audio_preview_media_id = media
        .items
        .iter()
        .find(|m| m.kind == MediaKind::AudioPreview)
        .map(|m| m.id.clone());
    let arrangement_image_media_id = media
        .items
        .iter()
        .find(|m| m.kind == MediaKind::ArrangementImage)
        .map(|m| m.id.clone());
    let title = summary.title.clone().unwrap_or_else(|| {
        summary
            .root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Untitled Project")
            .to_string()
    });

    Ok(ProjectCard {
        project_id: summary.project_id,
        root_path: summary.root,
        title,
        artists: summary.artist_aliases,
        genre: summary.genre,
        bpm: summary.bpm,
        root: summary.root_note,
        scale: summary.scale,
        lifecycle: summary.lifecycle,
        version_count: summary.version_count,
        channels: summary.channel_count,
        unfinished_tasks: summary.unfinished_tasks,
        last_activity: summary.updated_utc,
        has_audio_preview: summary.has_audio_preview,
        has_arrangement_image: summary.has_arrangement_image,
        audio_preview_media_id,
        arrangement_image_media_id,
        backup_state: backup_state(&paths).unwrap_or(BackupState::Unknown),
    })
}

/// Recursively discover VERSIONE projects without hashing audio or scanning ignored trees.
pub fn discover_projects(root: &Path, options: DiscoveryOptions) -> Result<Vec<DiscoveredProject>> {
    if !root.exists() {
        return Err(Error::new(ErrorKind::NotFound, "discovery root not found").with_path(root));
    }
    if !root.is_dir() {
        return Err(Error::new(
            ErrorKind::InvalidProject,
            "discovery root must be a directory",
        )
        .with_path(root));
    }

    let mut found = Vec::new();
    let mut queue = VecDeque::from([(root.to_path_buf(), 0usize)]);
    let mut seen_projects = HashSet::new();

    while let Some((dir, depth)) = queue.pop_front() {
        if depth > options.max_depth {
            continue;
        }
        if depth > 0 && should_skip_dir(&dir) {
            continue;
        }

        let paths = ProjectPaths::from_root(&dir);
        if paths.config_path().is_file() {
            let config = load_config(&paths)?;
            if (options.include_root || depth > 0)
                && seen_projects.insert(config.project_id.clone())
            {
                let title = crate::profile::load_or_default_profile(&paths)?.title;
                found.push(DiscoveredProject {
                    root_path: canonical_or_original(&dir),
                    project_id: config.project_id,
                    title,
                });
            }
            continue;
        }

        if depth == options.max_depth {
            continue;
        }
        let mut children = Vec::new();
        for entry in fs::read_dir(&dir)
            .map_err(|e| Error::io("failed to read discovery directory", &dir, e))?
        {
            let entry = entry.map_err(|e| Error::io("failed to read discovery entry", &dir, e))?;
            let path = entry.path();
            if path.is_dir() && !should_skip_dir(&path) {
                children.push(path);
            }
        }
        children.sort();
        queue.extend(children.into_iter().map(|child| (child, depth + 1)));
    }

    found.sort_by(|a, b| a.root_path.cmp(&b.root_path));
    Ok(found)
}

/// Discover and register every VERSIONE project under a folder.
pub fn add_folder_of_projects(root: &Path, options: DiscoveryOptions) -> Result<Vec<LibraryEntry>> {
    let mut entries = Vec::new();
    for project in discover_projects(root, options)? {
        entries.push(register_project(&project.root_path)?);
    }
    Ok(entries)
}

/// Update a registered ProjectId to a new root after verifying identity.
pub fn resolve_moved_project(project_id: &ProjectId, new_root: &Path) -> Result<MoveResolution> {
    let new_paths = ProjectPaths::from_root(new_root);
    let new_config = load_config(&new_paths)?;
    if &new_config.project_id != project_id {
        return Err(Error::new(
            ErrorKind::AlreadyExists,
            format!(
                "{:?}",
                ProjectIdConflict::NewRootIsDifferentProject {
                    expected: project_id.clone(),
                    actual: new_config.project_id,
                    root: new_root.to_path_buf(),
                }
            ),
        ));
    }

    let mut index = load_library()?;
    let new_root = canonical_or_original(new_root);
    let mut previous_root = None;
    for entry in &mut index.projects {
        if entry.project_id == project_id.to_string() {
            previous_root = Some(entry.root.clone());
            entry.root = new_root.clone();
            entry.registered_utc = Utc::now();
        } else if canonical_or_original(&entry.root) == new_root {
            return Err(Error::new(
                ErrorKind::AlreadyExists,
                format!(
                    "{:?}",
                    ProjectIdConflict::DuplicateRegisteredRoot {
                        project_id: project_id.clone(),
                        existing_root: entry.root.clone(),
                        new_root: new_root.clone(),
                    }
                ),
            ));
        }
    }
    if previous_root.is_none() {
        index.projects.push(LibraryEntry {
            root: new_root.clone(),
            project_id: project_id.to_string(),
            registered_utc: Utc::now(),
        });
    }
    save_library(&index)?;
    Ok(MoveResolution {
        project_id: project_id.clone(),
        previous_root,
        new_root,
    })
}

fn should_skip_dir(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    matches!(name, ".git" | "target" | "node_modules")
        || (name == "objects"
            && path
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                == Some(".versione"))
}

fn backup_state(paths: &ProjectPaths) -> Result<BackupState> {
    let storage = load_storage_config(paths)?;
    if storage.remote.is_none() {
        return Ok(BackupState::LocalOnly);
    }
    let publications = load_publication_index(paths)?;
    if publications
        .snapshots
        .iter()
        .any(|p| p.remote_verified && p.git_pushed)
    {
        return Ok(BackupState::Published);
    }
    if publications.snapshots.iter().any(|p| p.remote_verified) {
        return Ok(BackupState::RemoteVerified);
    }
    Ok(BackupState::RemoteConfigured)
}

fn canonical_or_original(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}
