//! VERSIONE core library.
//!
//! DAW-independent project metadata, content addressing, Git history integration,
//! pluggable large-object storage, and the Music Memory / workflow / insights layer.
//! Adapter-specific knowledge belongs outside this crate.

pub mod audio;
pub mod checkpoint;
pub mod classify;
pub mod collect;
pub mod daw;
pub mod diff;
pub mod enable;
pub mod error;
pub mod filesystem;
pub mod git;
pub mod history;
pub mod ignore;
pub mod insights;
pub mod library;
pub mod lifecycle;
pub mod lock;
pub mod manifest;
pub mod media;
pub mod metrics;
pub mod notes;
pub mod objects;
pub mod profile;
pub mod project;
pub mod project_root;
pub mod provenance;
pub mod publish;
pub mod restore;
pub mod scan;
pub mod services;
pub mod snapshot;
pub mod status;
pub mod storage;
pub mod tasks;
pub mod verify;
pub mod version;
pub mod workflow;

pub use checkpoint::{create_checkpoint, CheckpointOutcome, CheckpointReport};
pub use collect::{collect_into_media, CollectionOptions, CollectionReport};
pub use daw::reaper::{
    discover_references, inspect_rpp, prepare_reaper_self_contained, ReaperReference, ReaperReport,
    ReferenceClass, ReferenceKind,
};
pub use daw::{inspect_path, inspect_resolved, DawInspection, DawReferenceSummary};
pub use enable::{enable_project, snapshot_project, EnableOptions, EnableReport};
pub use error::{Error, ErrorKind, Result};
pub use history::{list_history, resolve_version_selector, HistoryEntry, HistoryEntryKind};
pub use insights::{
    compute_music_stats, compute_overview, music_from_summaries, overview_from_summaries,
    LibraryOverview, MusicStats, StatsWindow,
};
pub use library::{
    find_projects, list_library_summaries, rebuild_library, rediscovery_hints, register_project,
    FindQuery, LibraryIndex, RediscoveryHint,
};
pub use lifecycle::{derive_lifecycle, LifecyclePhase, ProjectLifecycle, LIFECYCLE_FORMAT_VERSION};
pub use media::{
    add_media_file, add_media_file_with_options, add_screenshot, list_media, list_screenshots,
    materialize_media_to_temp, screenshot_object_status, screenshots_for_snapshot, AddMediaOptions,
    AddScreenshotOptions, MediaKind, ProjectMedia,
};
pub use metrics::{
    latest_observation, list_observations, record_observation, MetricObservation, MetricValue,
};
pub use notes::{add_note, list_notes, show_note, ProjectNote};
pub use objects::{hash_file, hash_reader, ObjectId, HASH_ALGORITHM};
pub use profile::{
    load_or_default_profile, load_profile, project_summary, set_lifecycle, set_release_stage,
    update_profile, InstrumentUsage, MusicalInfo, ProjectProfile, ProjectSummary, SessionInfo,
};
pub use project::{
    configure_remote_storage, init_project, project_status, InitOptions, ProjectConfig, ProjectId,
    ProjectPaths, ProjectStatus, RemoteStorageConfig,
};
pub use project_root::{resolve_project_root, ProjectKind, ResolvedProjectRoot};
pub use provenance::{MetricSource, Provenance, ProvenancedValue};
pub use publish::{
    gate_git_push, publish_project, publish_with_remote_for_test, PublishOptions, PublishReport,
};
pub use restore::{restore_version_as_copy, RestoreReport};
pub use snapshot::SnapshotId;
pub use status::{rich_status, RichStatus};
pub use storage::{
    Availability, LocalObjectStore, MemoryObjectStore, ObjectStore, StorageLocationId,
};
pub use tasks::{
    add_task, complete_task, list_tasks, set_task_status, NewTask, ProjectTask, TaskPriority,
    TaskStatus,
};
pub use verify::{
    verify_manifest_on_disk, verify_working_tree, VerificationReport, VerifyFinding,
    VerifyIssueKind,
};
pub use version::{keep_version, VersionReport};
pub use workflow::{LifecycleState, ReleaseInfo, ReleaseStage};
