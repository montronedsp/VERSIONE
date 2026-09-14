//! Project tasks — a musician's completion list, not a project-management suite.

use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{Error, ErrorKind, Result};
use crate::git::GitRepository;
use crate::project::{read_toml, require_initialized, write_toml_atomic, ProjectPaths};
use crate::snapshot::SnapshotId;

pub const TASKS_FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskId(Uuid);

impl TaskId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for TaskId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for TaskId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum TaskStatus {
    #[default]
    Todo,
    Doing,
    Blocked,
    Done,
}

impl TaskStatus {
    pub fn parse(input: &str) -> Option<Self> {
        match input.trim().to_ascii_lowercase().as_str() {
            "todo" => Some(Self::Todo),
            "doing" => Some(Self::Doing),
            "blocked" => Some(Self::Blocked),
            "done" => Some(Self::Done),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Todo => "todo",
            Self::Doing => "doing",
            Self::Blocked => "blocked",
            Self::Done => "done",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum TaskPriority {
    Low,
    #[default]
    Normal,
    High,
}

impl TaskPriority {
    pub fn parse(input: &str) -> Option<Self> {
        match input.trim().to_ascii_lowercase().as_str() {
            "low" => Some(Self::Low),
            "normal" | "med" | "medium" => Some(Self::Normal),
            "high" => Some(Self::High),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectTask {
    pub id: TaskId,
    pub text: String,
    pub status: TaskStatus,
    #[serde(default)]
    pub priority: TaskPriority,
    pub created_utc: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_utc: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due_utc: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub related_snapshot_id: Option<SnapshotId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TasksDocument {
    pub format_version: u32,
    #[serde(default)]
    pub tasks: Vec<ProjectTask>,
}

impl TasksDocument {
    pub fn new_v1() -> Self {
        Self {
            format_version: TASKS_FORMAT_VERSION,
            tasks: Vec::new(),
        }
    }
}

impl ProjectPaths {
    pub fn tasks_path(&self) -> std::path::PathBuf {
        self.versione_dir.join("tasks.toml")
    }
}

pub fn load_or_default_tasks(paths: &ProjectPaths) -> Result<TasksDocument> {
    if !paths.tasks_path().exists() {
        return Ok(TasksDocument::new_v1());
    }
    let doc: TasksDocument = read_toml(&paths.tasks_path())?;
    if doc.format_version != TASKS_FORMAT_VERSION {
        return Err(Error::new(
            ErrorKind::UnsupportedFormat,
            format!("unsupported tasks format_version {}", doc.format_version),
        )
        .with_path(paths.tasks_path()));
    }
    Ok(doc)
}

pub fn save_tasks(paths: &ProjectPaths, doc: &TasksDocument) -> Result<()> {
    write_toml_atomic(&paths.tasks_path(), doc)
}

#[derive(Debug, Clone, Default)]
pub struct NewTask {
    pub text: String,
    pub priority: TaskPriority,
    pub category: Option<String>,
    pub due_utc: Option<DateTime<Utc>>,
    pub related_snapshot_id: Option<SnapshotId>,
}

pub fn add_task(root: &Path, new: NewTask) -> Result<ProjectTask> {
    if new.text.trim().is_empty() {
        return Err(Error::new(
            ErrorKind::InvalidProject,
            "task text is required",
        ));
    }
    let paths = require_initialized(root)?;
    let mut doc = load_or_default_tasks(&paths)?;
    let task = ProjectTask {
        id: TaskId::new(),
        text: new.text,
        status: TaskStatus::Todo,
        priority: new.priority,
        created_utc: Utc::now(),
        completed_utc: None,
        due_utc: new.due_utc,
        category: new.category,
        related_snapshot_id: new.related_snapshot_id,
    };
    doc.tasks.push(task.clone());
    doc.format_version = TASKS_FORMAT_VERSION;
    save_tasks(&paths, &doc)?;
    let git = GitRepository::new(root);
    let _ = git.commit_versione_metadata("Add project task")?;
    Ok(task)
}

pub fn list_tasks(root: &Path) -> Result<Vec<ProjectTask>> {
    let paths = require_initialized(root)?;
    Ok(load_or_default_tasks(&paths)?.tasks)
}

pub fn complete_task(root: &Path, id_prefix: &str) -> Result<ProjectTask> {
    set_task_status(root, id_prefix, TaskStatus::Done)
}

pub fn set_task_status(root: &Path, id_prefix: &str, status: TaskStatus) -> Result<ProjectTask> {
    let paths = require_initialized(root)?;
    let mut doc = load_or_default_tasks(&paths)?;
    let task = doc
        .tasks
        .iter_mut()
        .find(|t| t.id.to_string() == id_prefix || t.id.to_string().starts_with(id_prefix))
        .ok_or_else(|| Error::new(ErrorKind::NotFound, format!("task not found: {id_prefix}")))?;
    task.status = status;
    if status == TaskStatus::Done {
        task.completed_utc = Some(Utc::now());
    } else {
        task.completed_utc = None;
    }
    let result = task.clone();
    save_tasks(&paths, &doc)?;
    let git = GitRepository::new(root);
    let _ = git.commit_versione_metadata(&format!("Update task status to {}", status.as_str()))?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{init_project, InitOptions};
    use tempfile::tempdir;

    #[test]
    fn task_lifecycle() {
        let dir = tempdir().unwrap();
        init_project(dir.path(), &InitOptions::default()).unwrap();
        let task = add_task(
            dir.path(),
            NewTask {
                text: "Replace temporary snare".into(),
                priority: TaskPriority::High,
                category: Some("mix".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(task.status, TaskStatus::Todo);
        let done = complete_task(dir.path(), &task.id.to_string()[..8]).unwrap();
        assert_eq!(done.status, TaskStatus::Done);
        assert!(done.completed_utc.is_some());
    }
}
