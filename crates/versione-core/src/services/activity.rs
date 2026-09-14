//! Recent activity derived from profile, Versions, and tasks.

use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::library::list_library_summaries;
use crate::profile::load_or_default_profile;
use crate::project::{load_config, load_version_index, ProjectId, ProjectPaths};
use crate::tasks::load_or_default_tasks;

/// Coarse activity kind for Studio timelines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ActivityKind {
    ProfileUpdated,
    VersionCreated,
    TaskCreated,
    TaskCompleted,
}

/// Timeline event derived from durable project metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivityEvent {
    pub project_id: ProjectId,
    pub project_title: Option<String>,
    pub root_path: std::path::PathBuf,
    pub kind: ActivityKind,
    pub occurred_utc: DateTime<Utc>,
    pub summary: String,
}

/// Recent activity across the registered library.
pub fn recent_activity(limit: Option<usize>) -> Result<Vec<ActivityEvent>> {
    let mut events = Vec::new();
    for summary in list_library_summaries()? {
        events.extend(activity_for_project(&summary.root)?);
    }
    sort_events(&mut events);
    if let Some(limit) = limit {
        events.truncate(limit);
    }
    Ok(events)
}

/// Activity for one project, derived where metadata exists.
pub fn activity_for_project(root: &Path) -> Result<Vec<ActivityEvent>> {
    let paths = ProjectPaths::from_root(root);
    let config = load_config(&paths)?;
    let profile = load_or_default_profile(&paths)?;
    let title = profile.title.clone();
    let mut events = vec![ActivityEvent {
        project_id: config.project_id.clone(),
        project_title: title.clone(),
        root_path: paths.root.clone(),
        kind: ActivityKind::ProfileUpdated,
        occurred_utc: profile.updated_utc,
        summary: "Profile updated".into(),
    }];

    for version in load_version_index(&paths)?.versions {
        events.push(ActivityEvent {
            project_id: config.project_id.clone(),
            project_title: title.clone(),
            root_path: paths.root.clone(),
            kind: ActivityKind::VersionCreated,
            occurred_utc: version.created_utc,
            summary: format!("Created {}: {}", version.display, version.name),
        });
    }

    for task in load_or_default_tasks(&paths)?.tasks {
        events.push(ActivityEvent {
            project_id: config.project_id.clone(),
            project_title: title.clone(),
            root_path: paths.root.clone(),
            kind: ActivityKind::TaskCreated,
            occurred_utc: task.created_utc,
            summary: format!("Task created: {}", task.text),
        });
        if let Some(completed_utc) = task.completed_utc {
            events.push(ActivityEvent {
                project_id: config.project_id.clone(),
                project_title: title.clone(),
                root_path: paths.root.clone(),
                kind: ActivityKind::TaskCompleted,
                occurred_utc: completed_utc,
                summary: format!("Task completed: {}", task.text),
            });
        }
    }

    sort_events(&mut events);
    Ok(events)
}

fn sort_events(events: &mut [ActivityEvent]) {
    events.sort_by(|a, b| {
        b.occurred_utc
            .cmp(&a.occurred_utc)
            .then_with(|| a.project_id.to_string().cmp(&b.project_id.to_string()))
            .then_with(|| a.summary.cmp(&b.summary))
    });
}
