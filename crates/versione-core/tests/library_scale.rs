use std::fs;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use chrono::{Duration, Utc};
use tempfile::tempdir;
use versione_core::library::{self, LibraryEntry};
use versione_core::media::MediaDocument;
use versione_core::objects::HASH_ALGORITHM;
use versione_core::profile::ProjectProfile;
use versione_core::project::{
    ProjectConfig, ProjectPaths, ProjectState, StorageConfig, VersionIndex, VersionRecord,
    FORMAT_VERSION,
};
use versione_core::services::list_library_cards;
use versione_core::snapshot::SnapshotId;
use versione_core::tasks::{ProjectTask, TaskId, TaskPriority, TaskStatus, TasksDocument};
use versione_core::workflow::LifecycleState;
use versione_core::{LibraryIndex, ProjectId};

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[test]
fn lists_100_project_cards_fast() {
    let _guard = env_lock().lock().unwrap();
    let dir = tempdir().unwrap();
    seed_library(dir.path(), 100);

    let started = Instant::now();
    let cards = list_library_cards().unwrap();

    assert_eq!(cards.len(), 100);
    assert_eq!(cards[0].title, "Project 0000");
    eprintln!("100 project cards listed in {:?}", started.elapsed());
    std::env::remove_var("VERSIONE_LIBRARY");
}

#[test]
fn lists_1000_project_cards_fast() {
    let _guard = env_lock().lock().unwrap();
    let dir = tempdir().unwrap();
    seed_library(dir.path(), 1_000);

    let started = Instant::now();
    let cards = list_library_cards().unwrap();

    assert_eq!(cards.len(), 1_000);
    assert!(cards.iter().all(|card| card.version_count == 1));
    eprintln!("1000 project cards listed in {:?}", started.elapsed());
    std::env::remove_var("VERSIONE_LIBRARY");
}

#[test]
#[ignore]
fn bench_10000_project_cards() {
    let _guard = env_lock().lock().unwrap();
    let dir = tempdir().unwrap();
    seed_library(dir.path(), 10_000);

    let started = Instant::now();
    let cards = list_library_cards().unwrap();

    assert_eq!(cards.len(), 10_000);
    eprintln!("10000 project cards listed in {:?}", started.elapsed());
    std::env::remove_var("VERSIONE_LIBRARY");
}

fn seed_library(root: &Path, count: usize) {
    let library_path = root.join("library.toml");
    std::env::set_var("VERSIONE_LIBRARY", &library_path);
    let mut index = LibraryIndex::new_v1();

    for i in 0..count {
        let project_root = root.join(format!("project-{i:04}"));
        fs::create_dir_all(project_root.join(".versione/refs")).unwrap();
        fs::create_dir_all(project_root.join(".versione/objects")).unwrap();
        let project_id = ProjectId::new();
        write_project_metadata(&project_root, project_id.clone(), i);
        index.projects.push(LibraryEntry {
            root: project_root,
            project_id: project_id.to_string(),
            registered_utc: Utc::now(),
        });
    }

    library::save_library(&index).unwrap();
}

fn write_project_metadata(root: &Path, project_id: ProjectId, i: usize) {
    let paths = ProjectPaths::from_root(root);
    let created_utc = Utc::now() - Duration::days((i % 365) as i64);
    write_toml(
        &paths.config_path(),
        &ProjectConfig {
            format_version: FORMAT_VERSION,
            project_id: project_id.clone(),
            created_utc,
            hash_algorithm: HASH_ALGORITHM.into(),
        },
    );
    write_toml(
        &paths.storage_config_path(),
        &StorageConfig::new_v1_default(),
    );
    write_toml(&paths.state_path(), &ProjectState::new_v1());
    write_toml(
        &paths.versions_index_path(),
        &VersionIndex {
            format_version: FORMAT_VERSION,
            versions: vec![VersionRecord {
                number: 1,
                display: "V01".into(),
                name: "Seed".into(),
                snapshot_id: SnapshotId::new(),
                created_utc,
            }],
        },
    );

    let mut profile = ProjectProfile::new_for(project_id);
    profile.title = Some(format!("Project {i:04}"));
    profile.artist_aliases = vec!["Synthetic Artist".into()];
    profile.musical.genre = Some("Techno".into());
    profile.musical.bpm = Some(120.0 + (i % 30) as f64);
    profile.musical.root_note = Some("F".into());
    profile.musical.scale = Some("Minor".into());
    profile.lifecycle = Some(if i.is_multiple_of(5) {
        LifecycleState::Active
    } else {
        LifecycleState::CreativePool
    });
    profile.updated_utc = Utc::now() - Duration::seconds(i as i64);
    write_toml(&paths.profile_path(), &profile);

    let tasks = if i.is_multiple_of(10) {
        vec![ProjectTask {
            id: TaskId::new(),
            text: "Finish arrangement".into(),
            status: TaskStatus::Todo,
            priority: TaskPriority::Normal,
            created_utc,
            completed_utc: None,
            due_utc: None,
            category: Some("arrangement".into()),
            related_snapshot_id: None,
        }]
    } else {
        Vec::new()
    };
    write_toml(
        &paths.tasks_path(),
        &TasksDocument {
            format_version: versione_core::tasks::TASKS_FORMAT_VERSION,
            tasks,
        },
    );
    write_toml(&paths.media_path(), &MediaDocument::new_v1());
}

fn write_toml<T: serde::Serialize>(path: &Path, value: &T) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    let text = toml::to_string_pretty(value).unwrap();
    fs::write(path, text).unwrap();
}
