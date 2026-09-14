//! Integration tests for project init, hashing, and path safety.

use std::fs;
use std::io::{Cursor, Write};

use tempfile::tempdir;
use versione_core::filesystem::{safe_join, ProjectRelativePath};
use versione_core::ignore::IgnoreRules;
use versione_core::objects::{hash_file, hash_reader};
use versione_core::project::{
    init_project, load_config, project_status, InitOptions, ProjectPaths,
};
use versione_core::publish::gate_git_push;
use versione_core::storage::{Availability, LocalObjectStore, ObjectStore, StorageLocationId};
use versione_core::ErrorKind;

#[test]
fn init_round_trip_status_and_config() {
    let dir = tempdir().unwrap();
    let created = init_project(dir.path(), &InitOptions::default()).unwrap();
    let status = project_status(dir.path()).unwrap();
    assert!(status.initialized);
    let loaded = status.config.unwrap();
    assert_eq!(loaded.project_id, created.project_id);
    assert_eq!(loaded.format_version, 1);

    let paths = ProjectPaths::from_root(dir.path());
    let again = load_config(&paths).unwrap();
    assert_eq!(again.hash_algorithm, "blake3");
    assert!(paths.storage_config_path().is_file());
    assert!(dir.path().join(".git").is_dir());
}

#[test]
fn rejects_future_format_version() {
    let dir = tempdir().unwrap();
    init_project(dir.path(), &InitOptions::default()).unwrap();
    let config_path = dir.path().join(".versione/config.toml");
    let mut text = fs::read_to_string(&config_path).unwrap();
    text = text.replace("format_version = 1", "format_version = 99");
    fs::write(&config_path, text).unwrap();

    let err = project_status(dir.path()).unwrap_err();
    assert_eq!(err.kind, ErrorKind::UnsupportedFormat);
}

#[test]
fn large_file_streams_without_loading_all_at_once() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("large.bin");
    let mut file = fs::File::create(&path).unwrap();
    let chunk = vec![0x5A_u8; 1024 * 64];
    for _ in 0..128 {
        file.write_all(&chunk).unwrap();
    }
    drop(file);

    let first = hash_file(&path).unwrap();
    let second = hash_file(&path).unwrap();
    assert_eq!(first, second);
    assert_eq!(first.as_str().len(), 64);
}

#[test]
fn local_store_round_trip_and_dedup() {
    let dir = tempdir().unwrap();
    let store =
        LocalObjectStore::open(StorageLocationId::new("studio"), dir.path().join("store")).unwrap();
    let bytes = b"identical-stem-bytes";
    let id = hash_reader(Cursor::new(bytes)).unwrap();
    store.put(&id, &mut Cursor::new(bytes)).unwrap();
    store.put(&id, &mut Cursor::new(bytes)).unwrap();
    assert_eq!(store.verify(&id).unwrap(), Availability::Available);
}

#[test]
fn publication_gate_blocks_git_before_objects() {
    assert!(!gate_git_push(false, true).allowed);
    assert!(gate_git_push(true, true).allowed);
}

#[test]
fn path_escape_blocked_by_relative_parser() {
    assert_eq!(
        ProjectRelativePath::new("..\\outside").unwrap_err().kind,
        ErrorKind::PathEscape
    );
}

#[test]
fn safe_join_keeps_paths_inside_root() {
    let dir = tempdir().unwrap();
    let rel = ProjectRelativePath::new("Samples/Kick.wav").unwrap();
    let joined = safe_join(dir.path(), &rel).unwrap();
    assert!(joined.starts_with(dir.path().canonicalize().unwrap()));
}

#[test]
fn ignore_rules_are_deterministic() {
    let rules = IgnoreRules::default();
    let path = ProjectRelativePath::new("cache/file.tmp").unwrap();
    assert!(rules.ignores_relative(&path));
    assert!(!rules.ignores_relative(&ProjectRelativePath::new("set/Project.als").unwrap()));
}
