//! Enable VERSIONE on an existing music project.
//!
//! ```text
//! UNMANAGED → COLLECTED → VERIFIED → VERSIONED
//! ```
//!
//! The first snapshot is created only after collection and verification succeed.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::checkpoint::{create_checkpoint_locked, CheckpointOutcome};
use crate::collect::{collect_into_media, CollectionOptions, CollectionReport};
use crate::daw::reaper::prepare_reaper_self_contained;
use crate::daw::{inspect_resolved, DawReferenceSummary};
use crate::error::{Error, ErrorKind, Result};
use crate::lifecycle::{
    advance_lifecycle, derive_lifecycle, save_lifecycle_doc, LifecyclePhase, ProjectLifecycle,
};
use crate::lock::ProjectLock;
use crate::project::{init_project, load_config, InitOptions, ProjectId, ProjectPaths};
use crate::project_root::{resolve_project_root, ProjectKind};
use crate::verify::{verify_working_tree, VerificationReport};

const LOCK_STALE: Duration = Duration::from_secs(30 * 60);

#[derive(Debug, Clone, Default)]
pub struct EnableOptions {
    /// Extra external files to copy into `Media/imported/` (also used when DAW discovery is limited).
    pub external_paths: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct EnableReport {
    pub root: PathBuf,
    pub project_id: ProjectId,
    pub kind: ProjectKind,
    pub primary_project_file: Option<PathBuf>,
    pub lifecycle: ProjectLifecycle,
    pub collection: CollectionReport,
    pub verification: VerificationReport,
    pub checkpoint: Option<CheckpointOutcome>,
    pub daw_references: DawReferenceSummary,
    pub original_rpp_unchanged: Option<bool>,
    /// True when the project was already VERSIONED and enable was a no-op for history.
    pub already_enabled: bool,
}

/// Enable VERSIONE: resolve root → init → collect → verify → first snapshot.
pub fn enable_project(path: &Path, options: &EnableOptions) -> Result<EnableReport> {
    let resolved = resolve_project_root(path)?;
    let root = resolved.root.clone();

    let config = init_project(
        &root,
        &InitOptions {
            allow_existing: true,
        },
    )?;

    let paths = ProjectPaths::from_root(&root);
    let _lock = ProjectLock::acquire(&paths, LOCK_STALE)?;

    let current = derive_lifecycle(&paths)?;
    if matches!(
        current,
        ProjectLifecycle::Phase(LifecyclePhase::Versioned)
            | ProjectLifecycle::Phase(LifecyclePhase::Published)
    ) {
        let inspection = inspect_resolved(&resolved)?;
        let collection = collect_into_media(
            &root,
            &CollectionOptions {
                external_paths: Vec::new(),
                media_dir: None,
            },
        )?;
        let verification = verify_working_tree(&root)?;
        return Ok(EnableReport {
            root,
            project_id: config.project_id,
            kind: resolved.kind,
            primary_project_file: resolved.primary_project_file,
            lifecycle: current,
            collection,
            verification,
            checkpoint: None,
            daw_references: inspection.references,
            original_rpp_unchanged: None,
            already_enabled: true,
        });
    }

    let (collection, daw_references, original_rpp_unchanged) =
        prepare_collection(&root, &resolved, options)?;

    let mut life = advance_lifecycle(&paths, LifecyclePhase::Collected)?;
    life.collection_noop = collection.noop && options.external_paths.is_empty();
    save_lifecycle_doc(&paths, &life)?;

    let verification = verify_working_tree(&root)?;
    verification.require_ok()?;
    advance_lifecycle(&paths, LifecyclePhase::Verified)?;

    let checkpoint = create_checkpoint_locked(&paths)?;
    advance_lifecycle(&paths, LifecyclePhase::Versioned)?;

    let config = load_config(&paths)?;
    Ok(EnableReport {
        root,
        project_id: config.project_id,
        kind: resolved.kind,
        primary_project_file: resolved.primary_project_file,
        lifecycle: derive_lifecycle(&paths)?,
        collection,
        verification,
        checkpoint: Some(checkpoint),
        daw_references,
        original_rpp_unchanged,
        already_enabled: false,
    })
}

fn prepare_collection(
    root: &Path,
    resolved: &crate::project_root::ResolvedProjectRoot,
    options: &EnableOptions,
) -> Result<(CollectionReport, DawReferenceSummary, Option<bool>)> {
    match resolved.kind {
        ProjectKind::Reaper => {
            let rpp = resolved.primary_project_file.as_ref().ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidProject,
                    "REAPER project missing primary .rpp",
                )
            })?;
            let prepared = prepare_reaper_self_contained(root, rpp, &options.external_paths)?;
            if !prepared.original_rpp_unchanged {
                return Err(Error::new(
                    ErrorKind::Invariant,
                    "canonical .rpp changed during prepare; refusing to continue",
                ));
            }
            let summary = prepared.inspection.summary();
            // After collection, externals should be absorbed; collected count ≈ former external.
            let mut summary = summary;
            summary.collected = prepared.collection.copied + prepared.collection.reused_existing;
            Ok((
                prepared.collection,
                summary,
                Some(prepared.original_rpp_unchanged),
            ))
        }
        ProjectKind::Ableton | ProjectKind::Generic => {
            let inspection = inspect_resolved(resolved)?;
            let collection = collect_into_media(
                root,
                &CollectionOptions {
                    external_paths: options.external_paths.clone(),
                    media_dir: None,
                },
            )?;
            Ok((collection, inspection.references, None))
        }
    }
}

/// Subsequent snapshot with verification before commit.
pub fn snapshot_project(root: &Path) -> Result<CheckpointOutcome> {
    let paths = crate::project::require_initialized(root)?;
    let _lock = ProjectLock::acquire(&paths, LOCK_STALE)?;

    // Refresh REAPER self-contained staging when applicable.
    if let Ok(resolved) = resolve_project_root(root) {
        if resolved.kind == ProjectKind::Reaper {
            if let Some(rpp) = &resolved.primary_project_file {
                let _ = prepare_reaper_self_contained(root, rpp, &[])?;
            }
        }
    }

    let verification = verify_working_tree(&paths.root)?;
    verification.require_ok()?;

    match derive_lifecycle(&paths)? {
        ProjectLifecycle::Unmanaged => {
            return Err(Error::new(
                ErrorKind::InvalidProject,
                "VERSIONE is not enabled on this project",
            ));
        }
        ProjectLifecycle::Phase(LifecyclePhase::Collected) => {
            advance_lifecycle(&paths, LifecyclePhase::Verified)?;
        }
        ProjectLifecycle::Phase(
            LifecyclePhase::Verified | LifecyclePhase::Versioned | LifecyclePhase::Published,
        ) => {}
    }

    let outcome = create_checkpoint_locked(&paths)?;
    match derive_lifecycle(&paths)? {
        ProjectLifecycle::Phase(LifecyclePhase::Verified) => {
            advance_lifecycle(&paths, LifecyclePhase::Versioned)?;
        }
        ProjectLifecycle::Phase(LifecyclePhase::Versioned | LifecyclePhase::Published) => {}
        other => {
            return Err(Error::new(
                ErrorKind::Invariant,
                format!("unexpected lifecycle after snapshot: {other}"),
            ));
        }
    }
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::restore::restore_version_as_copy;
    use crate::version::keep_version;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn enable_valid_project_directory() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("My Track");
        fs::create_dir_all(root.join("Samples")).unwrap();
        fs::write(root.join("Track.als"), b"als-bytes").unwrap();
        fs::write(root.join("Samples/kick.wav"), b"RIFF....").unwrap();

        let report = enable_project(&root, &EnableOptions::default()).unwrap();
        assert!(!report.already_enabled);
        assert!(matches!(
            report.lifecycle,
            ProjectLifecycle::Phase(LifecyclePhase::Versioned)
        ));
        assert!(report.checkpoint.is_some());
        assert!(root.join(".versione/lifecycle.toml").is_file());
    }

    #[test]
    fn enable_via_als_path() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("Song");
        fs::create_dir_all(&root).unwrap();
        let als = root.join("Song.als");
        fs::write(&als, b"als").unwrap();
        let report = enable_project(&als, &EnableOptions::default()).unwrap();
        assert_eq!(report.kind, ProjectKind::Ableton);
        assert!(!report.already_enabled);
    }

    #[test]
    fn enable_already_enabled_is_idempotent() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("proj");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("a.txt"), b"x").unwrap();
        let first = enable_project(&root, &EnableOptions::default()).unwrap();
        let second = enable_project(&root, &EnableOptions::default()).unwrap();
        assert!(!first.already_enabled);
        assert!(second.already_enabled);
        assert_eq!(first.project_id, second.project_id);
    }

    #[test]
    fn enable_with_external_media() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("proj");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("Track.als"), b"als").unwrap();
        let external = dir.path().join("vocal.wav");
        fs::write(&external, b"WAVDATA123").unwrap();
        let report = enable_project(
            &root,
            &EnableOptions {
                external_paths: vec![external.clone()],
            },
        )
        .unwrap();
        assert_eq!(report.collection.copied, 1);
        assert!(external.is_file());
        assert!(root.join("Media/imported/vocal.wav").is_file());
    }

    #[test]
    fn second_snapshot_reuses_unchanged_objects() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("proj");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("Track.als"), b"als-v1").unwrap();
        fs::write(root.join("kick.wav"), b"RIFF-AUDIO-1").unwrap();
        enable_project(&root, &EnableOptions::default()).unwrap();
        fs::write(root.join("kick.wav"), b"RIFF-AUDIO-2-CHANGED").unwrap();
        match snapshot_project(&root).unwrap() {
            CheckpointOutcome::Created(r) => {
                assert_eq!(r.new_objects, 1, "only changed audio should be new");
                assert!(r.reused_objects >= 1, "als should be reused");
            }
            CheckpointOutcome::Unchanged { .. } => panic!("expected a new checkpoint"),
        }
    }

    #[test]
    fn enable_reaper_collects_and_leaves_original_rpp() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("reaper_proj");
        fs::create_dir_all(&root).unwrap();
        let outside = dir.path().join("samples");
        fs::create_dir_all(&outside).unwrap();
        let wav = outside.join("kick.wav");
        fs::write(&wav, b"RIFF-KICK").unwrap();
        let rpp = root.join("Song.rpp");
        let body = format!(
            "<REAPER_PROJECT 0.1 \"6.0\" 1\n  <TRACK\n    <ITEM\n      <SOURCE WAVE\n        FILE \"{}\"\n      >\n    >\n  >\n>\n",
            wav.display()
        );
        fs::write(&rpp, &body).unwrap();
        let report = enable_project(&rpp, &EnableOptions::default()).unwrap();
        assert_eq!(report.kind, ProjectKind::Reaper);
        assert_eq!(report.original_rpp_unchanged, Some(true));
        assert_eq!(fs::read_to_string(&rpp).unwrap(), body);
        assert!(root.join("Media/imported/kick.wav").is_file());
        let staged = fs::read_to_string(root.join(".versione/staging/Song.rpp")).unwrap();
        assert!(staged.contains("Media/imported/"));
    }

    #[test]
    fn reaper_restore_does_not_need_original_external() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("reaper_proj");
        fs::create_dir_all(&root).unwrap();
        let outside = dir.path().join("samples");
        fs::create_dir_all(&outside).unwrap();
        let wav = outside.join("kick.wav");
        fs::write(&wav, b"RIFF-KICK-RESTORE").unwrap();
        let rpp = root.join("Song.rpp");
        let body = format!(
            "<REAPER_PROJECT 0.1 \"6.0\" 1\n  <TRACK\n    <ITEM\n      <SOURCE WAVE\n        FILE \"{}\"\n      >\n    >\n  >\n>\n",
            wav.display()
        );
        fs::write(&rpp, &body).unwrap();
        enable_project(&rpp, &EnableOptions::default()).unwrap();
        keep_version(&root, "Take 1").unwrap();

        // Remove original external sample.
        fs::remove_file(&wav).unwrap();

        let dest = dir.path().join("restored");
        restore_version_as_copy(&root, "V01", &dest).unwrap();
        let restored_rpp = fs::read_to_string(dest.join("Song.rpp")).unwrap();
        assert!(
            restored_rpp.contains("Media/imported/"),
            "restored RPP must use project-local media paths"
        );
        assert!(!restored_rpp.contains(&wav.display().to_string()));
        assert!(dest.join("Media/imported/kick.wav").is_file());
    }
}
