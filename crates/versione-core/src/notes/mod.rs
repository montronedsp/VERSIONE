//! Project notes / documentation (versioned metadata).

use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{Error, ErrorKind, Result};
use crate::git::GitRepository;
use crate::project::{read_toml, require_initialized, write_toml_atomic, ProjectPaths};

pub const NOTES_FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteId(Uuid);

impl NoteId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for NoteId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for NoteId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Optional note category; free-form so users are not forced into rigid taxonomy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectNote {
    pub id: NoteId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    pub body: String,
    pub created_utc: DateTime<Utc>,
    pub updated_utc: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotesDocument {
    pub format_version: u32,
    #[serde(default)]
    pub notes: Vec<ProjectNote>,
}

impl NotesDocument {
    pub fn new_v1() -> Self {
        Self {
            format_version: NOTES_FORMAT_VERSION,
            notes: Vec::new(),
        }
    }
}

impl ProjectPaths {
    pub fn notes_path(&self) -> std::path::PathBuf {
        self.versione_dir.join("notes.toml")
    }
}

pub fn load_or_default_notes(paths: &ProjectPaths) -> Result<NotesDocument> {
    if !paths.notes_path().exists() {
        return Ok(NotesDocument::new_v1());
    }
    let doc: NotesDocument = read_toml(&paths.notes_path())?;
    if doc.format_version != NOTES_FORMAT_VERSION {
        return Err(Error::new(
            ErrorKind::UnsupportedFormat,
            format!("unsupported notes format_version {}", doc.format_version),
        )
        .with_path(paths.notes_path()));
    }
    Ok(doc)
}

pub fn save_notes(paths: &ProjectPaths, doc: &NotesDocument) -> Result<()> {
    write_toml_atomic(&paths.notes_path(), doc)
}

pub fn add_note(
    root: &Path,
    body: impl Into<String>,
    category: Option<String>,
) -> Result<ProjectNote> {
    let paths = require_initialized(root)?;
    let mut doc = load_or_default_notes(&paths)?;
    let now = Utc::now();
    let note = ProjectNote {
        id: NoteId::new(),
        category,
        body: body.into(),
        created_utc: now,
        updated_utc: now,
    };
    doc.notes.push(note.clone());
    doc.format_version = NOTES_FORMAT_VERSION;
    save_notes(&paths, &doc)?;
    let git = GitRepository::new(root);
    let _ = git.commit_versione_metadata("Add project note")?;
    Ok(note)
}

pub fn list_notes(root: &Path) -> Result<Vec<ProjectNote>> {
    let paths = require_initialized(root)?;
    Ok(load_or_default_notes(&paths)?.notes)
}

pub fn show_note(root: &Path, id: &str) -> Result<ProjectNote> {
    let notes = list_notes(root)?;
    notes
        .into_iter()
        .find(|n| n.id.to_string() == id || n.id.to_string().starts_with(id))
        .ok_or_else(|| Error::new(ErrorKind::NotFound, format!("note not found: {id}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{init_project, InitOptions};
    use tempfile::tempdir;

    #[test]
    fn note_add_list_persist() {
        let dir = tempdir().unwrap();
        init_project(dir.path(), &InitOptions::default()).unwrap();
        let note = add_note(dir.path(), "Fix kick/bass relationship", Some("mix".into())).unwrap();
        let listed = list_notes(dir.path()).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id.to_string(), note.id.to_string());
        let shown = show_note(dir.path(), &note.id.to_string()[..8]).unwrap();
        assert!(shown.body.contains("kick"));
    }
}
