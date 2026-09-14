//! Conservative collection into a project-local Media area.
//!
//! This is the self-containment phase of Enable VERSIONE. It does **not** parse
//! or rewrite Ableton `.als` Sets. External files are collected only when
//! supplied explicitly (Set reference discovery is a planned adapter).
//!
//! Collection copies; it never moves or deletes source files.

use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{Error, ErrorKind, Result};
use crate::filesystem::{safe_join, ProjectRelativePath};
use crate::objects::hash_file;
use crate::project::{read_toml, write_toml_atomic, ProjectPaths};

pub const COLLECTION_FORMAT_VERSION: u32 = 1;
pub const DEFAULT_MEDIA_DIR: &str = "Media";
pub const IMPORTED_SUBDIR: &str = "imported";

#[derive(Debug, Clone, Default)]
pub struct CollectionOptions {
    /// Explicit external files to copy into `Media/imported/`.
    pub external_paths: Vec<PathBuf>,
    /// Media directory name relative to project root.
    pub media_dir: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectedItem {
    pub source_path: String,
    pub destination_relative: String,
    pub object_id: String,
    pub size: u64,
    pub collected_utc: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CollectionDocument {
    pub format_version: u32,
    #[serde(default)]
    pub media_dir: String,
    #[serde(default)]
    pub items: Vec<CollectedItem>,
    #[serde(default)]
    pub unsupported_notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectionReport {
    pub media_dir: PathBuf,
    pub copied: usize,
    pub reused_existing: usize,
    pub skipped_identical: usize,
    pub items: Vec<CollectedItem>,
    pub unsupported_notes: Vec<String>,
    pub noop: bool,
}

impl ProjectPaths {
    pub fn collection_path(&self) -> PathBuf {
        self.versione_dir.join("collection.toml")
    }
}

/// Run collection for an initialized project.
pub fn collect_into_media(root: &Path, options: &CollectionOptions) -> Result<CollectionReport> {
    let paths = ProjectPaths::from_root(root);
    if !paths.config_path().exists() {
        return Err(Error::new(
            ErrorKind::InvalidProject,
            "VERSIONE is not initialized; run enable/init first",
        )
        .with_path(root));
    }

    let media_name = options
        .media_dir
        .clone()
        .unwrap_or_else(|| DEFAULT_MEDIA_DIR.to_string());
    if media_name.contains("..") || Path::new(&media_name).is_absolute() {
        return Err(Error::new(
            ErrorKind::PathEscape,
            "media_dir must be a relative path without '..'",
        ));
    }

    let media_root = root.join(&media_name);
    let imported = media_root.join(IMPORTED_SUBDIR);
    fs::create_dir_all(&imported)
        .map_err(|e| Error::io("failed to create Media/imported", &imported, e))?;

    let mut doc = load_collection_doc(&paths)?.unwrap_or(CollectionDocument {
        format_version: COLLECTION_FORMAT_VERSION,
        media_dir: media_name.clone(),
        items: Vec::new(),
        unsupported_notes: Vec::new(),
    });
    doc.media_dir = media_name.clone();

    let mut unsupported_notes = Vec::new();
    unsupported_notes.push(
        "Ableton Set parsing is not used for reference discovery yet; supply external files explicitly or rely on assets already inside the project."
            .into(),
    );

    let mut copied = 0usize;
    let mut reused_existing = 0usize;
    let mut skipped_identical = 0usize;
    let mut new_items = Vec::new();

    for external in &options.external_paths {
        match collect_one_external(root, &imported, &doc, external)? {
            CollectOutcome::Copied(item) => {
                copied += 1;
                doc.items.push(item.clone());
                new_items.push(item);
            }
            CollectOutcome::Reused(item) => {
                reused_existing += 1;
                if !doc
                    .items
                    .iter()
                    .any(|i| i.destination_relative == item.destination_relative)
                {
                    doc.items.push(item);
                }
            }
            CollectOutcome::SkippedIdentical => skipped_identical += 1,
        }
    }

    let already = count_project_audio(root)?;
    if already > 0 {
        unsupported_notes.push(format!(
            "{already} audio file(s) already inside the project tree were left in place (no move)."
        ));
    }

    doc.unsupported_notes = unsupported_notes.clone();
    doc.format_version = COLLECTION_FORMAT_VERSION;
    write_toml_atomic(&paths.collection_path(), &doc)?;

    let noop = copied == 0 && options.external_paths.is_empty();
    Ok(CollectionReport {
        media_dir: media_root,
        copied,
        reused_existing,
        skipped_identical,
        items: new_items,
        unsupported_notes,
        noop,
    })
}

enum CollectOutcome {
    Copied(CollectedItem),
    Reused(CollectedItem),
    SkippedIdentical,
}

fn collect_one_external(
    root: &Path,
    imported: &Path,
    doc: &CollectionDocument,
    external: &Path,
) -> Result<CollectOutcome> {
    if !external.is_file() {
        return Err(
            Error::new(ErrorKind::NotFound, "external media file not found").with_path(external),
        );
    }
    if let Ok(canon_ext) = fs::canonicalize(external) {
        if let Ok(canon_root) = fs::canonicalize(root) {
            if canon_ext.starts_with(&canon_root) {
                return Err(Error::new(
                    ErrorKind::InvalidProject,
                    "external path is already inside the project; collection copies only outside assets",
                )
                .with_path(external));
            }
        }
    }

    let object_id = hash_file(external)?;
    let size = fs::metadata(external)
        .map_err(|e| Error::io("failed to stat external media", external, e))?
        .len();

    if let Some(existing) = doc.items.iter().find(|i| i.object_id == object_id.as_str()) {
        let dest = root.join(&existing.destination_relative);
        if dest.is_file() {
            let got = hash_file(&dest)?;
            if got == object_id {
                return Ok(CollectOutcome::SkippedIdentical);
            }
        }
    }

    let original_name = external
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "media.bin".into());
    let dest_name = unique_dest_name(imported, &original_name, object_id.as_str())?;
    let dest_abs = imported.join(&dest_name);

    if dest_abs.exists() {
        let existing = hash_file(&dest_abs)?;
        if existing == object_id {
            let rel = path_relative(root, &dest_abs)?;
            return Ok(CollectOutcome::Reused(CollectedItem {
                source_path: redact_source(external),
                destination_relative: rel,
                object_id: object_id.to_string(),
                size,
                collected_utc: Utc::now(),
            }));
        }
    }

    copy_verified(external, &dest_abs, &object_id)?;
    let rel = path_relative(root, &dest_abs)?;
    Ok(CollectOutcome::Copied(CollectedItem {
        source_path: redact_source(external),
        destination_relative: rel,
        object_id: object_id.to_string(),
        size,
        collected_utc: Utc::now(),
    }))
}

fn unique_dest_name(imported: &Path, original: &str, object_id: &str) -> Result<String> {
    let candidate = imported.join(original);
    if !candidate.exists() {
        return Ok(original.to_string());
    }
    let existing = hash_file(&candidate)?;
    if existing.as_str() == object_id {
        return Ok(original.to_string());
    }
    let (stem, ext) = split_name(original);
    Ok(format!("{stem}-{}.{ext}", &object_id[..8]))
}

fn split_name(name: &str) -> (String, String) {
    match name.rfind('.') {
        Some(i) if i > 0 => (name[..i].to_string(), name[i + 1..].to_string()),
        _ => (name.to_string(), "bin".into()),
    }
}

fn copy_verified(src: &Path, dest: &Path, expected: &crate::objects::ObjectId) -> Result<()> {
    let tmp = dest.with_extension("versione-partial");
    {
        let in_file =
            File::open(src).map_err(|e| Error::io("failed to open external media", src, e))?;
        let out_file = File::create(&tmp)
            .map_err(|e| Error::io("failed to create temp media copy", &tmp, e))?;
        let mut reader = BufReader::with_capacity(64 * 1024, in_file);
        let mut writer = BufWriter::with_capacity(64 * 1024, out_file);
        std::io::copy(&mut reader, &mut writer).map_err(|e| {
            Error::new(ErrorKind::Io, "failed copying external media").with_source(e)
        })?;
        writer
            .flush()
            .map_err(|e| Error::io("failed flushing media copy", &tmp, e))?;
    }
    let got = hash_file(&tmp)?;
    if &got != expected {
        let _ = fs::remove_file(&tmp);
        return Err(Error::new(
            ErrorKind::Corruption,
            "copied media hash mismatch; refusing to finalize collection",
        ));
    }
    fs::rename(&tmp, dest).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        Error::io("failed to finalize collected media", dest, e)
    })?;
    Ok(())
}

fn path_relative(root: &Path, abs: &Path) -> Result<String> {
    let rel = abs.strip_prefix(root).map_err(|_| {
        Error::new(ErrorKind::PathEscape, "collected path escaped project root").with_path(abs)
    })?;
    Ok(rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/"))
}

fn redact_source(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "unknown".into())
}

fn count_project_audio(root: &Path) -> Result<usize> {
    let mut count = 0usize;
    fn walk(dir: &Path, count: &mut usize) -> Result<()> {
        for entry in fs::read_dir(dir).map_err(|e| Error::io("failed reading dir", dir, e))? {
            let entry = entry.map_err(|e| Error::io("failed reading entry", dir, e))?;
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if name == ".git" || name == ".versione" {
                continue;
            }
            let ft = entry
                .file_type()
                .map_err(|e| Error::io("failed file type", &path, e))?;
            if ft.is_symlink() {
                continue;
            }
            if ft.is_dir() {
                walk(&path, count)?;
            } else if ft.is_file() {
                let ext = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                if matches!(
                    ext.as_str(),
                    "wav" | "aif" | "aiff" | "flac" | "mp3" | "ogg" | "rex" | "rx2"
                ) {
                    *count += 1;
                }
            }
        }
        Ok(())
    }
    walk(root, &mut count)?;
    Ok(count)
}

pub fn load_collection_doc(paths: &ProjectPaths) -> Result<Option<CollectionDocument>> {
    if !paths.collection_path().exists() {
        return Ok(None);
    }
    let doc: CollectionDocument = read_toml(&paths.collection_path())?;
    if doc.format_version != COLLECTION_FORMAT_VERSION {
        return Err(Error::new(
            ErrorKind::UnsupportedFormat,
            format!(
                "unsupported collection format_version {}",
                doc.format_version
            ),
        )
        .with_path(paths.collection_path()));
    }
    Ok(Some(doc))
}

/// Refuse path traversal into Media via crafted names.
pub fn media_join(root: &Path, media_dir: &str, relative: &str) -> Result<PathBuf> {
    let base_rel = ProjectRelativePath::new(media_dir)?;
    let base = safe_join(root, &base_rel)?;
    let child = ProjectRelativePath::new(relative)?;
    safe_join(&base, &child)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{init_project, InitOptions};
    use tempfile::tempdir;

    #[test]
    fn copies_external_without_moving_source() {
        let dir = tempdir().unwrap();
        let project = dir.path().join("proj");
        fs::create_dir_all(&project).unwrap();
        init_project(&project, &InitOptions::default()).unwrap();
        let external = dir.path().join("outside.wav");
        fs::write(&external, b"RIFFWAVE-DATA").unwrap();
        let report = collect_into_media(
            &project,
            &CollectionOptions {
                external_paths: vec![external.clone()],
                media_dir: None,
            },
        )
        .unwrap();
        assert_eq!(report.copied, 1);
        assert!(external.is_file());
        assert!(project.join("Media/imported/outside.wav").is_file());
    }

    #[test]
    fn collision_with_different_content_gets_distinct_name() {
        let dir = tempdir().unwrap();
        let project = dir.path().join("proj");
        fs::create_dir_all(&project).unwrap();
        init_project(&project, &InitOptions::default()).unwrap();
        let media = project.join("Media/imported");
        fs::create_dir_all(&media).unwrap();
        fs::write(media.join("hit.wav"), b"AAAA").unwrap();
        let external = dir.path().join("hit.wav");
        fs::write(&external, b"BBBB").unwrap();
        let report = collect_into_media(
            &project,
            &CollectionOptions {
                external_paths: vec![external],
                media_dir: None,
            },
        )
        .unwrap();
        assert_eq!(report.copied, 1);
        assert_ne!(
            report.items[0].destination_relative,
            "Media/imported/hit.wav"
        );
    }

    #[test]
    fn rejects_path_traversal_in_media_join() {
        let dir = tempdir().unwrap();
        let err = media_join(dir.path(), "Media", "../escape.wav").unwrap_err();
        assert_eq!(err.kind, ErrorKind::PathEscape);
    }
}
