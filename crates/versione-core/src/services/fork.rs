//! Project fork service.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{Error, ErrorKind, Result};
use crate::profile::{load_or_default_profile, update_profile};
use crate::project::{init_project, load_config, InitOptions, ProjectId, ProjectPaths};

/// Options for creating a project fork.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ForkOptions {
    pub title: Option<String>,
    pub extra_tags: Vec<String>,
}

/// Result of a fork operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForkReport {
    pub source_project_id: ProjectId,
    pub fork_project_id: ProjectId,
    pub source_root: PathBuf,
    pub fork_root: PathBuf,
    pub created_utc: DateTime<Utc>,
}

/// Copy project files to a new root and initialize a fresh ProjectId.
///
/// `.versione` and `.git` are intentionally not copied. The fork records parent
/// provenance in profile description/tags.
pub fn fork_project(
    source_root: &Path,
    fork_root: &Path,
    options: ForkOptions,
) -> Result<ForkReport> {
    let source_paths = crate::project::require_initialized(source_root)?;
    let source_config = load_config(&source_paths)?;
    let source_profile = load_or_default_profile(&source_paths)?;

    if fork_root.exists() {
        return Err(
            Error::new(ErrorKind::AlreadyExists, "fork destination already exists")
                .with_path(fork_root),
        );
    }
    fs::create_dir_all(fork_root)
        .map_err(|e| Error::io("failed to create fork destination", fork_root, e))?;
    copy_project_tree(source_root, fork_root)?;

    let fork_config = init_project(fork_root, &InitOptions::default())?;
    let created_utc = Utc::now();
    let parent_note = format!(
        "Forked from VERSIONE project {} at {}.",
        source_config.project_id,
        source_paths.root.display()
    );

    update_profile(fork_root, |profile| {
        let mut next = source_profile.clone();
        next.project_id = fork_config.project_id.clone();
        next.title = options.title.clone().or(next.title);
        next.description = match next.description.take() {
            Some(existing) if !existing.trim().is_empty() => {
                Some(format!("{existing}\n\n{parent_note}"))
            }
            _ => Some(parent_note.clone()),
        };
        add_unique_tag(&mut next.tags, "fork");
        add_unique_tag(
            &mut next.tags,
            &format!("fork-of:{}", source_config.project_id),
        );
        for tag in &options.extra_tags {
            add_unique_tag(&mut next.tags, tag);
        }
        *profile = next;
    })?;

    Ok(ForkReport {
        source_project_id: source_config.project_id,
        fork_project_id: fork_config.project_id,
        source_root: source_paths.root,
        fork_root: ProjectPaths::from_root(fork_root).root,
        created_utc,
    })
}

fn copy_project_tree(source: &Path, dest: &Path) -> Result<()> {
    let mut entries = fs::read_dir(source)
        .map_err(|e| Error::io("failed to read project directory", source, e))?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| Error::io("failed to read project entry", source, e))?;
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let path = entry.path();
        if should_skip(&path) {
            continue;
        }
        let target = dest.join(entry.file_name());
        if path.is_dir() {
            fs::create_dir_all(&target)
                .map_err(|e| Error::io("failed to create fork directory", &target, e))?;
            copy_project_tree(&path, &target)?;
        } else if path.is_file() {
            fs::copy(&path, &target)
                .map_err(|e| Error::io("failed to copy fork file", &path, e))?;
        }
    }
    Ok(())
}

fn should_skip(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|n| n.to_str()),
        Some(".versione" | ".git")
    )
}

fn add_unique_tag(tags: &mut Vec<String>, tag: &str) {
    if tag.trim().is_empty() {
        return;
    }
    if !tags.iter().any(|existing| existing == tag) {
        tags.push(tag.to_string());
    }
}
