//! Focus board composition for VERSIONE Studio.

use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::project::ProjectId;
use crate::tasks::{load_or_default_tasks, ProjectTask, TaskStatus};
use crate::workflow::{LifecycleState, ReleaseStage};

use super::library_service::{list_library_cards, ProjectCard};

/// Task row with enough project context for a Studio board.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenTaskCard {
    pub project_id: ProjectId,
    pub project_title: String,
    pub root_path: std::path::PathBuf,
    pub task: ProjectTask,
}

/// Musician-facing focus board sections.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct FocusBoard {
    pub active: Vec<ProjectCard>,
    pub needs_attention: Vec<ProjectCard>,
    pub almost_finished: Vec<ProjectCard>,
    pub open_tasks: Vec<OpenTaskCard>,
    pub dormant: Vec<ProjectCard>,
    pub publishing_candidates: Vec<ProjectCard>,
}

pub fn focus_board() -> Result<FocusBoard> {
    let cards = list_library_cards()?;
    let dormant_cutoff = Utc::now() - Duration::days(180);
    let mut board = FocusBoard::default();

    for card in &cards {
        if matches!(
            card.lifecycle,
            Some(LifecycleState::Active | LifecycleState::Publishing)
        ) {
            board.active.push(card.clone());
        }
        if card.unfinished_tasks > 0 || !card.has_audio_preview {
            board.needs_attention.push(card.clone());
        }
        if card.version_count >= 2
            && card.unfinished_tasks <= 2
            && !matches!(
                card.lifecycle,
                Some(LifecycleState::Released | LifecycleState::Archived)
            )
        {
            board.almost_finished.push(card.clone());
        }
        if card.last_activity <= dormant_cutoff
            && !matches!(
                card.lifecycle,
                Some(LifecycleState::Released | LifecycleState::Archived)
            )
        {
            board.dormant.push(card.clone());
        }
        if card.version_count > 0
            && matches!(
                card.lifecycle,
                Some(LifecycleState::Active | LifecycleState::Publishing)
            )
        {
            board.publishing_candidates.push(card.clone());
        }

        let paths = crate::project::ProjectPaths::from_root(&card.root_path);
        if let Ok(tasks) = load_or_default_tasks(&paths) {
            for task in tasks.tasks {
                if task.status != TaskStatus::Done {
                    board.open_tasks.push(OpenTaskCard {
                        project_id: card.project_id.clone(),
                        project_title: card.title.clone(),
                        root_path: card.root_path.clone(),
                        task,
                    });
                }
            }
        }
    }

    board.open_tasks.sort_by(|a, b| {
        task_rank(&a.task)
            .cmp(&task_rank(&b.task))
            .then_with(|| a.project_title.cmp(&b.project_title))
            .then_with(|| a.task.created_utc.cmp(&b.task.created_utc))
    });
    board.publishing_candidates.sort_by(|a, b| {
        stage_rank(a)
            .cmp(&stage_rank(b))
            .then_with(|| b.version_count.cmp(&a.version_count))
            .then_with(|| a.title.cmp(&b.title))
    });
    Ok(board)
}

fn task_rank(task: &ProjectTask) -> i32 {
    match task.status {
        TaskStatus::Blocked => 0,
        TaskStatus::Doing => 1,
        TaskStatus::Todo => 2,
        TaskStatus::Done => 3,
    }
}

fn stage_rank(card: &ProjectCard) -> i32 {
    match card.lifecycle {
        Some(LifecycleState::Publishing) => 0,
        Some(LifecycleState::Active) => 1,
        _ => match card.version_count {
            0 => 3,
            _ => 2,
        },
    }
}

#[allow(dead_code)]
fn release_stage_rank(stage: Option<ReleaseStage>) -> i32 {
    match stage {
        Some(ReleaseStage::Candidate) | None => 0,
        Some(ReleaseStage::Selected) => 1,
        Some(ReleaseStage::Mixing) => 2,
        Some(ReleaseStage::Mastering) => 3,
        Some(ReleaseStage::Artwork) => 4,
        Some(ReleaseStage::Metadata) => 5,
        Some(ReleaseStage::Submitted) => 6,
        Some(ReleaseStage::Scheduled) => 7,
        Some(ReleaseStage::Released) => 8,
    }
}
