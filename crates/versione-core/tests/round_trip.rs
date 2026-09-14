//! End-to-end local checkpoint / Version / restore round-trip.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use tempfile::tempdir;
use versione_core::manifest::SnapshotManifest;
use versione_core::objects::hash_file;
use versione_core::project::{init_project, load_state, InitOptions, ProjectPaths};
use versione_core::storage::{LocalObjectStore, ObjectStore, StorageLocationId};
use versione_core::{create_checkpoint, keep_version, restore_version_as_copy, CheckpointOutcome};

fn write_file(path: &Path, bytes: &[u8]) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, bytes).unwrap();
}

fn tree_hashes(root: &Path) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    fn walk(base: &Path, dir: &Path, map: &mut BTreeMap<String, String>) {
        for entry in fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            let name = entry.file_name();
            if name == ".versione" || name == ".git" {
                continue;
            }
            if path.is_dir() {
                walk(base, &path, map);
            } else {
                let rel = path
                    .strip_prefix(base)
                    .unwrap()
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("/");
                map.insert(rel, hash_file(&path).unwrap().to_string());
            }
        }
    }
    walk(root, root, &mut map);
    map
}

fn object_count(store_root: &Path) -> usize {
    let mut count = 0usize;
    fn walk(dir: &Path, count: &mut usize) {
        if !dir.exists() {
            return;
        }
        for entry in fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.is_dir() {
                walk(&path, count);
            } else if path.is_file() {
                *count += 1;
            }
        }
    }
    walk(store_root, &mut count);
    count
}

#[test]
fn end_to_end_version_round_trip_byte_perfect() {
    let dir = tempdir().unwrap();
    let project = dir.path().join("Project");
    fs::create_dir_all(project.join("Samples")).unwrap();

    write_file(&project.join("song.als"), b"ALS-STATE-A");
    write_file(&project.join("Notes.txt"), b"session notes");
    write_file(&project.join("Samples/kick.wav"), b"KICK-WAV-BYTES-AAAA");
    write_file(&project.join("Samples/bass.wav"), b"BASS-WAV-BYTES-AAAA");

    init_project(&project, &InitOptions::default()).unwrap();

    let v01 = keep_version(&project, "First good groove").unwrap();
    assert_eq!(v01.display, "V01");
    let state_a = tree_hashes(&project);
    let paths = ProjectPaths::from_root(&project);
    let objects_before = object_count(&paths.versione_dir.join("objects"));

    // Unchanged checkpoint should not duplicate history/objects.
    match create_checkpoint(&project).unwrap() {
        CheckpointOutcome::Unchanged { .. } => {}
        CheckpointOutcome::Created(_) => panic!("expected unchanged checkpoint"),
    }

    // Modify project toward state B.
    write_file(&project.join("song.als"), b"ALS-STATE-B");
    write_file(&project.join("Samples/bass.wav"), b"BASS-WAV-BYTES-BBBB");
    write_file(&project.join("Samples/hat.wav"), b"HAT-WAV-BYTES-NEW");
    fs::remove_file(project.join("Notes.txt")).unwrap();

    let v02 = keep_version(&project, "Arrangement").unwrap();
    assert_eq!(v02.display, "V02");
    let state_b = tree_hashes(&project);

    let manifest_v01 = SnapshotManifest::load(&paths.manifest_path(&v01.snapshot_id)).unwrap();
    let manifest_v02 = SnapshotManifest::load(&paths.manifest_path(&v02.snapshot_id)).unwrap();

    let kick_v01 = manifest_v01
        .entries
        .iter()
        .find(|e| e.path == "Samples/kick.wav")
        .unwrap();
    let kick_v02 = manifest_v02
        .entries
        .iter()
        .find(|e| e.path == "Samples/kick.wav")
        .unwrap();
    assert_eq!(kick_v01.object_id, kick_v02.object_id);

    let bass_v01 = manifest_v01
        .entries
        .iter()
        .find(|e| e.path == "Samples/bass.wav")
        .unwrap();
    let bass_v02 = manifest_v02
        .entries
        .iter()
        .find(|e| e.path == "Samples/bass.wav")
        .unwrap();
    assert_ne!(bass_v01.object_id, bass_v02.object_id);

    assert!(manifest_v02
        .entries
        .iter()
        .any(|e| e.path == "Samples/hat.wav"));
    assert!(!manifest_v02.entries.iter().any(|e| e.path == "Notes.txt"));
    assert!(manifest_v01.entries.iter().any(|e| e.path == "Notes.txt"));

    let objects_after = object_count(&paths.versione_dir.join("objects"));
    assert!(objects_after > objects_before);
    // kick unchanged => not duplicated as a second object file.
    let store = LocalObjectStore::open(
        StorageLocationId::new("local"),
        paths.versione_dir.join("objects"),
    )
    .unwrap();
    assert!(store.has(&kick_v01.object_id).unwrap());

    let restored_v01 = dir.path().join("Restored-V01");
    let restored_v02 = dir.path().join("Restored-V02");
    restore_version_as_copy(&project, "V01", &restored_v01).unwrap();
    restore_version_as_copy(&project, "V02", &restored_v02).unwrap();

    assert_eq!(tree_hashes(&restored_v01), state_a);
    assert_eq!(tree_hashes(&restored_v02), state_b);

    // Original project still at state B and untouched by restore.
    assert_eq!(tree_hashes(&project), state_b);
    assert!(!project.join("Notes.txt").exists());

    let state = load_state(&paths).unwrap();
    assert_eq!(state.current_version, Some(v02.snapshot_id));
}

#[test]
fn duplicate_checkpoint_is_noop() {
    let dir = tempdir().unwrap();
    let project = dir.path().join("P");
    fs::create_dir_all(&project).unwrap();
    write_file(&project.join("a.txt"), b"hello");
    init_project(&project, &InitOptions::default()).unwrap();
    assert!(matches!(
        create_checkpoint(&project).unwrap(),
        CheckpointOutcome::Created(_)
    ));
    assert!(matches!(
        create_checkpoint(&project).unwrap(),
        CheckpointOutcome::Unchanged { .. }
    ));
}

#[test]
fn restore_refuses_existing_destination() {
    let dir = tempdir().unwrap();
    let project = dir.path().join("P");
    fs::create_dir_all(&project).unwrap();
    write_file(&project.join("a.txt"), b"hello");
    init_project(&project, &InitOptions::default()).unwrap();
    keep_version(&project, "One").unwrap();
    let dest = dir.path().join("out");
    fs::create_dir_all(&dest).unwrap();
    let err = restore_version_as_copy(&project, "V01", &dest).unwrap_err();
    assert_eq!(err.kind, versione_core::ErrorKind::AlreadyExists);
}

#[test]
fn unicode_paths_round_trip() {
    let dir = tempdir().unwrap();
    let project = dir.path().join("P");
    fs::create_dir_all(project.join("Samples")).unwrap();
    write_file(&project.join("Samples/kick_üñîçødé.wav"), b"UNICODE-WAV");
    init_project(&project, &InitOptions::default()).unwrap();
    keep_version(&project, "Unicode take").unwrap();
    let dest = dir.path().join("R");
    restore_version_as_copy(&project, "V01", &dest).unwrap();
    assert_eq!(
        hash_file(&project.join("Samples/kick_üñîçødé.wav")).unwrap(),
        hash_file(&dest.join("Samples/kick_üñîçødé.wav")).unwrap()
    );
}

#[allow(dead_code)]
fn touch(path: PathBuf) {
    write_file(&path, b"x");
}
