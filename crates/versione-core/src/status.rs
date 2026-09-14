//! Rich project status for the development CLI.

use std::path::Path;

use crate::classify::ClassifyPolicy;
use crate::error::Result;
use crate::git::{GitRepository, GitStatus};
use crate::ignore::IgnoreRules;
use crate::manifest::SnapshotManifest;
use crate::objects::ObjectId;
use crate::project::{
    load_config, load_publication_index, load_state, load_storage_config, load_version_index,
    project_status, require_initialized, VersionRecord,
};
use crate::scan::scan_project;
use crate::snapshot::SnapshotId;
use crate::storage::{Availability, ObjectStore};

/// Expanded musician-facing status.
#[derive(Debug, Clone)]
pub struct RichStatus {
    pub root: std::path::PathBuf,
    pub initialized: bool,
    pub project_id: Option<String>,
    pub enablement: crate::lifecycle::ProjectLifecycle,
    pub project_kind: Option<crate::project_root::ProjectKind>,
    pub project_file: Option<String>,
    pub daw_references: Option<crate::daw::DawReferenceSummary>,
    pub self_contained_note: Option<String>,
    pub unresolved_external_notes: Vec<String>,
    pub current_version: Option<VersionRecord>,
    pub current_checkpoint: Option<SnapshotId>,
    pub checkpoint_needed: bool,
    pub tracked_files: usize,
    pub modified_or_new: usize,
    pub deleted_since_checkpoint: usize,
    pub missing_objects: usize,
    pub corrupt_objects: usize,
    pub local_store_ok: bool,
    pub remote_configured: bool,
    pub remote_verified_current: bool,
    pub git_published_current: bool,
    pub git: GitStatus,
}

pub fn rich_status(root: &Path) -> Result<RichStatus> {
    let basic = project_status(root)?;
    if !basic.initialized {
        return Ok(RichStatus {
            root: basic.root,
            initialized: false,
            project_id: None,
            enablement: crate::lifecycle::ProjectLifecycle::Unmanaged,
            project_kind: None,
            project_file: None,
            daw_references: None,
            self_contained_note: None,
            unresolved_external_notes: Vec::new(),
            current_version: None,
            current_checkpoint: None,
            checkpoint_needed: false,
            tracked_files: 0,
            modified_or_new: 0,
            deleted_since_checkpoint: 0,
            missing_objects: 0,
            corrupt_objects: 0,
            local_store_ok: false,
            remote_configured: false,
            remote_verified_current: false,
            git_published_current: false,
            git: GitRepository::new(root).detect_status().unwrap_or_default(),
        });
    }

    let paths = require_initialized(root)?;
    let config = load_config(&paths)?;
    let state = load_state(&paths)?;
    let index = load_version_index(&paths)?;
    let current_version = state.current_version.as_ref().and_then(|id| {
        index
            .versions
            .iter()
            .find(|v| &v.snapshot_id == id)
            .cloned()
    });

    let ignore = IgnoreRules::default();
    let policy = ClassifyPolicy::default();
    let scanned = scan_project(&paths.root, &ignore, &policy)?;
    let tracked_files = scanned.len();

    let mut modified_or_new = 0usize;
    let mut deleted_since_checkpoint = 0usize;
    let mut missing_objects = 0usize;
    let mut corrupt_objects = 0usize;
    let mut local_store_ok = true;
    let mut checkpoint_needed = state.current_checkpoint.is_none();

    let store = match paths.open_local_store() {
        Ok(store) => Some(store),
        Err(_) => {
            local_store_ok = false;
            None
        }
    };

    if let Some(checkpoint_id) = &state.current_checkpoint {
        let manifest_path = paths.manifest_path(checkpoint_id);
        if manifest_path.exists() {
            let manifest = SnapshotManifest::load(&manifest_path)?;
            let scan_map: std::collections::BTreeMap<_, _> = scanned
                .iter()
                .map(|f| {
                    let content =
                        crate::daw::reaper::resolve_content_path(&paths, f.relative.as_str())
                            .unwrap_or_else(|_| f.absolute.clone());
                    let id = crate::objects::hash_file(&content).ok();
                    let size = std::fs::metadata(&content)
                        .map(|m| m.len())
                        .unwrap_or(f.size);
                    (f.relative.as_str().to_string(), (size, id))
                })
                .collect();

            for entry in &manifest.entries {
                match scan_map.get(&entry.path) {
                    None => deleted_since_checkpoint += 1,
                    Some((size, Some(id))) if *size != entry.size || id != &entry.object_id => {
                        modified_or_new += 1;
                    }
                    Some((_, None)) => modified_or_new += 1,
                    Some(_) => {}
                }
            }
            for path in scan_map.keys() {
                if !manifest.entries.iter().any(|e| &e.path == path) {
                    modified_or_new += 1;
                }
            }
            checkpoint_needed = modified_or_new > 0 || deleted_since_checkpoint > 0;

            if let Some(store) = &store {
                for entry in &manifest.entries {
                    tally_object(
                        store,
                        &entry.object_id,
                        &mut missing_objects,
                        &mut corrupt_objects,
                    )?;
                }
            }
        } else {
            checkpoint_needed = true;
            local_store_ok = false;
        }
    }

    let git = GitRepository::new(root).detect_status()?;
    let storage = load_storage_config(&paths)?;
    let remote_configured = storage.remote.is_some();
    let publication = load_publication_index(&paths)?;
    let current_id = state
        .current_version
        .clone()
        .or_else(|| state.current_checkpoint.clone());
    let current_pub = current_id.and_then(|id| {
        publication
            .snapshots
            .into_iter()
            .find(|r| r.snapshot_id == id)
    });

    let enablement = crate::lifecycle::derive_lifecycle(&paths)?;
    let collection = crate::collect::load_collection_doc(&paths)?;
    let self_contained_note = collection.as_ref().map(|c| {
        if c.items.is_empty() {
            "Collection record present; no external assets were copied by VERSIONE.".into()
        } else {
            format!(
                "{} external asset(s) recorded under {}/",
                c.items.len(),
                c.media_dir
            )
        }
    });
    let unresolved_external_notes = collection.map(|c| c.unsupported_notes).unwrap_or_default();

    let (project_kind, project_file, daw_references) =
        match crate::project_root::resolve_project_root(&paths.root) {
            Ok(resolved) => {
                let file = resolved.primary_project_file.as_ref().map(|p| {
                    p.file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| p.display().to_string())
                });
                let refs = crate::daw::inspect_resolved(&resolved)
                    .ok()
                    .map(|i| i.references);
                (Some(resolved.kind), file, refs)
            }
            Err(_) => (None, None, None),
        };

    Ok(RichStatus {
        root: paths.root,
        initialized: true,
        project_id: Some(config.project_id.to_string()),
        enablement,
        project_kind,
        project_file,
        daw_references,
        self_contained_note,
        unresolved_external_notes,
        current_version,
        current_checkpoint: state.current_checkpoint,
        checkpoint_needed,
        tracked_files,
        modified_or_new,
        deleted_since_checkpoint,
        missing_objects,
        corrupt_objects,
        local_store_ok,
        remote_configured,
        remote_verified_current: current_pub
            .as_ref()
            .map(|p| p.remote_verified)
            .unwrap_or(false),
        git_published_current: current_pub.map(|p| p.git_pushed).unwrap_or(false),
        git,
    })
}

fn tally_object(
    store: &dyn ObjectStore,
    id: &ObjectId,
    missing: &mut usize,
    corrupt: &mut usize,
) -> Result<()> {
    match store.verify(id)? {
        Availability::Available => {}
        Availability::Missing => *missing += 1,
        Availability::Corrupt => *corrupt += 1,
        Availability::Unknown => *missing += 1,
    }
    Ok(())
}
