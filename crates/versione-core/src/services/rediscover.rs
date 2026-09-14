//! Rediscovery candidates ranked from stored metadata only.

use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::library::{list_library_summaries, matches_query, FindQuery};
use crate::project::ProjectId;
use crate::workflow::LifecycleState;

use super::library_service::{card_from_root, ProjectCard};

/// Explanation for why a project appears in rediscovery.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RediscoverReason {
    pub code: String,
    pub explanation: String,
    pub score: i32,
}

/// A deterministic rediscovery suggestion. Higher scores rank first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RediscoverCandidate {
    pub project_id: ProjectId,
    pub card: ProjectCard,
    pub reasons: Vec<RediscoverReason>,
    pub score: i32,
}

/// Return ranked candidates. Passing a query narrows the candidate set first.
pub fn rediscover_candidates(
    query: Option<&FindQuery>,
    limit: Option<usize>,
) -> Result<Vec<RediscoverCandidate>> {
    let now = Utc::now();
    let mut candidates = Vec::new();

    for summary in list_library_summaries()? {
        if let Some(query) = query {
            if !matches_query(&summary, query) {
                continue;
            }
        }

        let mut reasons = Vec::new();
        if summary.lifecycle == Some(LifecycleState::CreativePool) {
            reasons.push(RediscoverReason {
                code: "creative-pool".into(),
                explanation: "Project is intentionally kept in the Creative Pool.".into(),
                score: 20,
            });
        }
        if summary.unfinished_tasks > 0 {
            reasons.push(RediscoverReason {
                code: "open-tasks".into(),
                explanation: format!(
                    "{} unfinished task(s) are recorded.",
                    summary.unfinished_tasks
                ),
                score: 12 + summary.unfinished_tasks.min(8) as i32,
            });
        }
        if summary.version_count >= 3
            && !matches!(
                summary.lifecycle,
                Some(LifecycleState::Released | LifecycleState::Archived)
            )
        {
            reasons.push(RediscoverReason {
                code: "version-depth".into(),
                explanation: format!(
                    "{} named Versions suggest developed material.",
                    summary.version_count
                ),
                score: 18,
            });
        }
        if !summary.has_audio_preview {
            reasons.push(RediscoverReason {
                code: "missing-preview".into(),
                explanation: "No audio preview is attached yet.".into(),
                score: 5,
            });
        }
        if now.signed_duration_since(summary.updated_utc) >= Duration::days(180)
            && summary.lifecycle != Some(LifecycleState::Archived)
        {
            reasons.push(RediscoverReason {
                code: "dormant".into(),
                explanation: "Project has not been updated for at least 180 days.".into(),
                score: 10,
            });
        }

        if reasons.is_empty() {
            continue;
        }
        let score = reasons.iter().map(|r| r.score).sum();
        candidates.push(RediscoverCandidate {
            project_id: summary.project_id,
            card: card_from_root(&summary.root)?,
            reasons,
            score,
        });
    }

    candidates.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| a.card.title.cmp(&b.card.title))
            .then_with(|| a.project_id.to_string().cmp(&b.project_id.to_string()))
    });
    if let Some(limit) = limit {
        candidates.truncate(limit);
    }
    Ok(candidates)
}

/// Explicit no-op hook for a UI skip action. It never mutates project metadata.
pub fn skip_rediscover_candidate(_project_id: &ProjectId) -> Result<()> {
    Ok(())
}
