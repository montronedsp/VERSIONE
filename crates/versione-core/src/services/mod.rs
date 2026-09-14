//! Application-service APIs for VERSIONE Studio.
//!
//! These services compose core metadata into UI-ready boards without adding
//! adapter-specific DAW knowledge.

pub mod activity;
pub mod focus;
pub mod fork;
pub mod library_service;
pub mod publishing;
pub mod rediscover;

pub use activity::{activity_for_project, recent_activity, ActivityEvent, ActivityKind};
pub use focus::{focus_board, FocusBoard, OpenTaskCard};
pub use fork::{fork_project, ForkOptions, ForkReport};
pub use library_service::{
    add_folder_of_projects, discover_projects, list_library_cards, resolve_moved_project,
    BackupState, DiscoveredProject, DiscoveryOptions, MoveResolution, ProjectCard,
    ProjectIdConflict,
};
pub use publishing::{
    pipeline_board, release_checklist, PipelineBoard, ReleaseChecklistItem, ReleaseStageColumn,
};
pub use rediscover::{
    rediscover_candidates, skip_rediscover_candidate, RediscoverCandidate, RediscoverReason,
};
