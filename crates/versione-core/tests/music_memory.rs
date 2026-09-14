//! Music Memory migration / compatibility tests.

use std::fs;

use tempfile::tempdir;
use versione_core::{
    create_checkpoint, init_project, load_or_default_profile, project_summary, update_profile,
    CheckpointOutcome, InitOptions, LifecycleState, ProjectPaths,
};

#[test]
fn phase1_projects_open_without_profile_using_defaults() {
    let dir = tempdir().unwrap();
    init_project(dir.path(), &InitOptions::default()).unwrap();
    // Simulate older project: no profile.toml yet.
    let paths = ProjectPaths::from_root(dir.path());
    assert!(!paths.profile_path().exists());

    let profile = load_or_default_profile(&paths).unwrap();
    assert!(profile.lifecycle.is_none());
    assert!(profile.title.is_none());

    let summary = project_summary(dir.path()).unwrap();
    assert_eq!(summary.project_id, profile.project_id);
    assert_eq!(summary.unfinished_tasks, 0);
}

#[test]
fn checkpoint_restore_invariants_survive_profile_updates() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    init_project(root, &InitOptions::default()).unwrap();
    fs::write(root.join("session.txt"), b"take-1").unwrap();
    let CheckpointOutcome::Created(report) = create_checkpoint(root).unwrap() else {
        panic!("expected created checkpoint");
    };
    let id_before = report.snapshot_id;

    update_profile(root, |p| {
        p.title = Some("After checkpoint".into());
        p.lifecycle = Some(LifecycleState::Active);
    })
    .unwrap();

    // Content checkpoint tip unchanged by metadata-only revision.
    let state = versione_core::project::load_state(&ProjectPaths::from_root(root)).unwrap();
    assert_eq!(state.current_checkpoint, Some(id_before));
}

#[test]
fn project_id_stable_across_directory_rename() {
    let dir = tempdir().unwrap();
    let original = dir.path().join("original");
    fs::create_dir_all(&original).unwrap();
    let config = init_project(&original, &InitOptions::default()).unwrap();
    let id = config.project_id.clone();
    update_profile(&original, |p| {
        p.title = Some("Rename me".into());
    })
    .unwrap();

    let moved = dir.path().join("moved-project");
    fs::rename(&original, &moved).unwrap();
    let profile = load_or_default_profile(&ProjectPaths::from_root(&moved)).unwrap();
    assert_eq!(profile.project_id, id);
    assert_eq!(profile.title.as_deref(), Some("Rename me"));
}
