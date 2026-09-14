//! Publishing pipeline service for Studio.

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::library::list_library_summaries;
use crate::profile::{load_or_default_profile, ProjectSummary};
use crate::project::ProjectPaths;
use crate::workflow::{ReleaseInfo, ReleaseStage};

use super::library_service::{card_from_root, ProjectCard};

/// A release checklist item derived from a concrete `ReleaseInfo` field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseChecklistItem {
    pub field: String,
    pub label: String,
    pub complete: bool,
}

/// Projects grouped by release pipeline stage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReleaseStageColumn {
    pub stage: ReleaseStage,
    pub projects: Vec<ProjectCard>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PipelineBoard {
    pub stages: Vec<ReleaseStageColumn>,
}

pub fn pipeline_board() -> Result<PipelineBoard> {
    let summaries = list_library_summaries()?;
    let mut stages = release_stages()
        .iter()
        .copied()
        .map(|stage| ReleaseStageColumn {
            stage,
            projects: Vec::new(),
        })
        .collect::<Vec<_>>();

    for summary in summaries {
        let paths = ProjectPaths::from_root(&summary.root);
        let profile = load_or_default_profile(&paths)?;
        let stage = profile.release.stage.unwrap_or(ReleaseStage::Candidate);
        if let Some(column) = stages.iter_mut().find(|column| column.stage == stage) {
            column.projects.push(card_from_root(&summary.root)?);
        }
    }

    for column in &mut stages {
        column.projects.sort_by(|a, b| {
            b.last_activity
                .cmp(&a.last_activity)
                .then_with(|| a.title.cmp(&b.title))
        });
    }
    Ok(PipelineBoard { stages })
}

/// Build a release checklist from actual `ReleaseInfo` fields only.
pub fn release_checklist(info: &ReleaseInfo) -> Vec<ReleaseChecklistItem> {
    vec![
        item("intended_artist", "Artist", &info.intended_artist),
        item("release_title", "Release title", &info.release_title),
        item("track_title", "Track title", &info.track_title),
        item("label", "Label", &info.label),
        item("catalog_number", "Catalog number", &info.catalog_number),
        item("release_format", "Release format", &info.release_format),
        item(
            "target_release_date",
            "Target release date",
            &info.target_release_date,
        ),
        item(
            "actual_release_date",
            "Actual release date",
            &info.actual_release_date,
        ),
        item("distributor", "Distributor", &info.distributor),
        item(
            "mastering_status",
            "Mastering status",
            &info.mastering_status,
        ),
        item("artwork_status", "Artwork status", &info.artwork_status),
        item("metadata_status", "Metadata status", &info.metadata_status),
        item("isrc", "ISRC", &info.isrc),
        item("upc_ean", "UPC/EAN", &info.upc_ean),
        item("promo_status", "Promo status", &info.promo_status),
        item(
            "submission_status",
            "Submission status",
            &info.submission_status,
        ),
        item("release_notes", "Release notes", &info.release_notes),
    ]
}

pub(crate) fn release_stages() -> &'static [ReleaseStage] {
    &[
        ReleaseStage::Candidate,
        ReleaseStage::Selected,
        ReleaseStage::Mixing,
        ReleaseStage::Mastering,
        ReleaseStage::Artwork,
        ReleaseStage::Metadata,
        ReleaseStage::Submitted,
        ReleaseStage::Scheduled,
        ReleaseStage::Released,
    ]
}

fn item(field: &str, label: &str, value: &Option<String>) -> ReleaseChecklistItem {
    ReleaseChecklistItem {
        field: field.into(),
        label: label.into(),
        complete: value
            .as_ref()
            .map(|v| !v.trim().is_empty())
            .unwrap_or(false),
    }
}

#[allow(dead_code)]
fn stage_for_summary(summary: &ProjectSummary) -> ReleaseStage {
    summary.release_stage.unwrap_or(ReleaseStage::Candidate)
}
