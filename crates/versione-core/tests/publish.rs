//! Publication and remote object-store integration tests.

use std::fs;
use std::path::Path;
use std::sync::Arc;

use tempfile::tempdir;
use versione_core::storage::{MemoryObjectStore, ObjectStore, StorageLocationId};
use versione_core::{
    create_checkpoint, init_project, keep_version, publish_project, CheckpointOutcome, InitOptions,
    PublishOptions,
};

fn write_file(path: &Path, bytes: &[u8]) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, bytes).unwrap();
}

fn setup_project(root: &Path) {
    fs::create_dir_all(root.join("Samples")).unwrap();
    write_file(&root.join("song.als"), b"ALS-A");
    write_file(&root.join("Samples/kick.wav"), b"KICK-BYTES");
    write_file(&root.join("Samples/bass.wav"), b"BASS-BYTES");
    init_project(root, &InitOptions::default()).unwrap();
    keep_version(root, "First").unwrap();
}

#[test]
fn publish_uploads_all_objects_before_marking_verified() {
    let dir = tempdir().unwrap();
    let project = dir.path().join("P");
    setup_project(&project);

    let remote = MemoryObjectStore::new(StorageLocationId::new("remote"));
    let report = publish_project(
        &project,
        PublishOptions {
            selector: Some("V01".into()),
            remote_override: Some(Arc::new(remote)),
            skip_git_push: true,
        },
    )
    .unwrap();

    assert!(report.remote_verified);
    assert!(!report.git_pushed);
    assert!(report.objects_uploaded >= 3);
}

#[test]
fn publish_is_idempotent_and_reuses_remote_objects() {
    let dir = tempdir().unwrap();
    let project = dir.path().join("P");
    setup_project(&project);
    let remote = Arc::new(MemoryObjectStore::new(StorageLocationId::new("remote")));

    let first = publish_project(
        &project,
        PublishOptions {
            selector: Some("V01".into()),
            remote_override: Some(remote.clone()),
            skip_git_push: true,
        },
    )
    .unwrap();
    let second = publish_project(
        &project,
        PublishOptions {
            selector: Some("V01".into()),
            remote_override: Some(remote.clone()),
            skip_git_push: true,
        },
    )
    .unwrap();

    assert!(first.objects_uploaded > 0);
    assert_eq!(second.objects_uploaded, 0);
    assert!(second.objects_reused > 0);
    assert!(second.remote_verified);
}

#[test]
fn upload_failure_blocks_publication() {
    let dir = tempdir().unwrap();
    let project = dir.path().join("P");
    setup_project(&project);

    let remote = MemoryObjectStore::new(StorageLocationId::new("remote"));
    // Fail the first object that will be uploaded.
    let paths = versione_core::project::ProjectPaths::from_root(&project);
    let state = versione_core::project::load_state(&paths).unwrap();
    let snap = state.current_version.unwrap();
    let manifest =
        versione_core::manifest::SnapshotManifest::load(&paths.manifest_path(&snap)).unwrap();
    remote.set_fail_put_for(Some(manifest.entries[0].object_id.clone()));

    let err = publish_project(
        &project,
        PublishOptions {
            selector: Some("V01".into()),
            remote_override: Some(Arc::new(remote)),
            skip_git_push: true,
        },
    )
    .unwrap_err();
    assert_eq!(err.kind, versione_core::ErrorKind::Storage);
}

#[test]
fn stale_cached_publication_cannot_skip_remote_verify() {
    let dir = tempdir().unwrap();
    let project = dir.path().join("P");
    setup_project(&project);
    let remote = Arc::new(MemoryObjectStore::new(StorageLocationId::new("remote")));

    let _ = publish_project(
        &project,
        PublishOptions {
            selector: Some("V01".into()),
            remote_override: Some(remote.clone()),
            skip_git_push: true,
        },
    )
    .unwrap();

    // Hide remote objects after a prior successful publish record exists.
    remote.set_hide_existing(true);
    let err = publish_project(
        &project,
        PublishOptions {
            selector: Some("V01".into()),
            remote_override: Some(remote),
            skip_git_push: true,
        },
    )
    .unwrap_err();
    assert!(matches!(
        err.kind,
        versione_core::ErrorKind::NotFound
            | versione_core::ErrorKind::Publication
            | versione_core::ErrorKind::Storage
    ));
}

#[test]
fn dedup_across_versions_on_remote() {
    let dir = tempdir().unwrap();
    let project = dir.path().join("P");
    setup_project(&project);
    let remote = Arc::new(MemoryObjectStore::new(StorageLocationId::new("remote")));

    let first = publish_project(
        &project,
        PublishOptions {
            selector: Some("V01".into()),
            remote_override: Some(remote.clone()),
            skip_git_push: true,
        },
    )
    .unwrap();

    write_file(&project.join("song.als"), b"ALS-B");
    keep_version(&project, "Second").unwrap();
    let second = publish_project(
        &project,
        PublishOptions {
            selector: Some("V02".into()),
            remote_override: Some(remote.clone()),
            skip_git_push: true,
        },
    )
    .unwrap();

    assert!(first.objects_uploaded >= 3);
    // Unchanged wav objects should be reused remotely.
    assert!(second.objects_reused >= 2);
    assert!(second.objects_uploaded >= 1);
    // Total unique remote objects should be less than naive sum of uploads without dedup
    // across both publishes' upload counts when reuse is counted separately.
    assert!(second.objects_reused > 0);
    assert_eq!(
        remote.len() as u64,
        first.objects_uploaded + second.objects_uploaded
    );
}

#[test]
fn gate_blocks_when_remote_incomplete() {
    assert!(!versione_core::gate_git_push(false, true).allowed);
}

#[test]
fn phase1_checkpoint_still_works_alongside_publish_types() {
    let dir = tempdir().unwrap();
    let project = dir.path().join("P");
    fs::create_dir_all(&project).unwrap();
    write_file(&project.join("a.txt"), b"x");
    init_project(&project, &InitOptions::default()).unwrap();
    assert!(matches!(
        create_checkpoint(&project).unwrap(),
        CheckpointOutcome::Created(_)
    ));
}

#[test]
fn memory_store_round_trip() {
    use std::io::{Cursor, Read};
    use versione_core::hash_reader;

    let store = MemoryObjectStore::new(StorageLocationId::new("mem"));
    let bytes = b"remote-payload";
    let id = hash_reader(Cursor::new(bytes)).unwrap();
    store.put(&id, &mut Cursor::new(bytes)).unwrap();
    assert!(store.has(&id).unwrap());
    let mut out = Vec::new();
    store.get(&id).unwrap().read_to_end(&mut out).unwrap();
    assert_eq!(out, bytes);
}
