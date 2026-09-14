//! Project media attachments (previews) backed by the object store.
//!
//! Binary payloads never enter Git. Only metadata references (`object_id`) are committed.

use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{Error, ErrorKind, Result};
use crate::git::GitRepository;
use crate::objects::{hash_file, ObjectId};
use crate::project::{read_toml, require_initialized, write_toml_atomic, ProjectPaths};
use crate::snapshot::SnapshotId;
use crate::storage::ObjectStore;

pub const MEDIA_FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaId(Uuid);

impl MediaId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for MediaId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for MediaId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    AudioPreview,
    ArrangementImage,
    /// Optional visual memory attached to a snapshot (not required for reconstruction).
    Screenshot,
    Reference,
    Artwork,
    Document,
    Other,
}

impl MediaKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AudioPreview => "audio_preview",
            Self::ArrangementImage => "arrangement_image",
            Self::Screenshot => "screenshot",
            Self::Reference => "reference",
            Self::Artwork => "artwork",
            Self::Document => "document",
            Self::Other => "other",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectMedia {
    pub id: MediaId,
    pub kind: MediaKind,
    pub object_id: ObjectId,
    pub mime_type: String,
    pub created_utc: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub related_snapshot_id: Option<SnapshotId>,
    pub byte_size: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_filename: Option<String>,
    /// DAW/project kind at attach time (portable label; not a filesystem path).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_kind: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaDocument {
    pub format_version: u32,
    #[serde(default)]
    pub items: Vec<ProjectMedia>,
}

impl MediaDocument {
    pub fn new_v1() -> Self {
        Self {
            format_version: MEDIA_FORMAT_VERSION,
            items: Vec::new(),
        }
    }
}

impl ProjectPaths {
    pub fn media_path(&self) -> std::path::PathBuf {
        self.versione_dir.join("media.toml")
    }
}

pub fn load_or_default_media(paths: &ProjectPaths) -> Result<MediaDocument> {
    if !paths.media_path().exists() {
        return Ok(MediaDocument::new_v1());
    }
    let doc: MediaDocument = read_toml(&paths.media_path())?;
    if doc.format_version != MEDIA_FORMAT_VERSION {
        return Err(Error::new(
            ErrorKind::UnsupportedFormat,
            format!("unsupported media format_version {}", doc.format_version),
        )
        .with_path(paths.media_path()));
    }
    Ok(doc)
}

pub fn save_media(paths: &ProjectPaths, doc: &MediaDocument) -> Result<()> {
    write_toml_atomic(&paths.media_path(), doc)
}

fn guess_mime(path: &Path, kind: MediaKind) -> String {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match (kind, ext.as_str()) {
        (MediaKind::AudioPreview, "wav") => "audio/wav".into(),
        (MediaKind::AudioPreview, "flac") => "audio/flac".into(),
        (MediaKind::AudioPreview, "mp3") => "audio/mpeg".into(),
        (MediaKind::AudioPreview, "aiff" | "aif") => "audio/aiff".into(),
        (MediaKind::ArrangementImage | MediaKind::Artwork | MediaKind::Screenshot, "png") => {
            "image/png".into()
        }
        (
            MediaKind::ArrangementImage | MediaKind::Artwork | MediaKind::Screenshot,
            "jpg" | "jpeg",
        ) => "image/jpeg".into(),
        (MediaKind::ArrangementImage | MediaKind::Artwork | MediaKind::Screenshot, "webp") => {
            "image/webp".into()
        }
        (_, _) => "application/octet-stream".into(),
    }
}

/// Options when attaching media / screenshots.
#[derive(Debug, Clone, Default)]
pub struct AddMediaOptions {
    pub label: Option<String>,
    pub related_snapshot_id: Option<SnapshotId>,
    pub project_kind: Option<String>,
}

/// Attach a preview file: store bytes in the object store, record metadata only in Git.
pub fn add_media_file(
    root: &Path,
    file: &Path,
    kind: MediaKind,
    label: Option<String>,
) -> Result<ProjectMedia> {
    add_media_file_with_options(
        root,
        file,
        kind,
        &AddMediaOptions {
            label,
            ..Default::default()
        },
    )
}

/// Attach media with optional snapshot association (screenshots / attachments).
pub fn add_media_file_with_options(
    root: &Path,
    file: &Path,
    kind: MediaKind,
    options: &AddMediaOptions,
) -> Result<ProjectMedia> {
    if !file.is_file() {
        return Err(Error::new(ErrorKind::NotFound, "preview file not found").with_path(file));
    }
    let paths = require_initialized(root)?;
    let store = paths.open_local_store()?;
    let object_id = hash_file(file)?;
    let meta = {
        let mut reader = BufReader::new(
            File::open(file).map_err(|e| Error::io("failed to open preview file", file, e))?,
        );
        store.put(&object_id, &mut reader)?
    };
    if store.verify(&object_id)? != crate::storage::Availability::Available {
        return Err(Error::new(
            ErrorKind::Corruption,
            "media object failed verification after put",
        ));
    }

    let item = ProjectMedia {
        id: MediaId::new(),
        kind,
        object_id: object_id.clone(),
        mime_type: guess_mime(file, kind),
        created_utc: Utc::now(),
        label: options.label.clone(),
        description: None,
        related_snapshot_id: options.related_snapshot_id.clone(),
        byte_size: meta.size,
        // Basename only — never store absolute machine paths in metadata.
        source_filename: file.file_name().map(|n| n.to_string_lossy().into_owned()),
        project_kind: options.project_kind.clone(),
    };

    let mut doc = load_or_default_media(&paths)?;
    // Replace prior primary preview of the same kind when attaching a new one.
    // Screenshots accumulate (one or more per snapshot) and must not clobber history.
    if matches!(kind, MediaKind::AudioPreview | MediaKind::ArrangementImage) {
        doc.items.retain(|m| m.kind != kind);
    }
    doc.items.push(item.clone());
    doc.format_version = MEDIA_FORMAT_VERSION;
    save_media(&paths, &doc)?;

    let git = GitRepository::new(root);
    let _ = git.commit_versione_metadata(&format!("Add project media ({})", kind.as_str()))?;

    if !store.has(&object_id)? {
        return Err(Error::new(
            ErrorKind::Invariant,
            "media object missing from object store after put",
        ));
    }

    Ok(item)
}

/// Import an optional screenshot and associate it with a snapshot when possible.
///
/// Screenshots enrich visual history; they are never part of musical snapshot identity.
pub fn add_screenshot(
    root: &Path,
    image: &Path,
    options: &AddScreenshotOptions,
) -> Result<ProjectMedia> {
    let paths = require_initialized(root)?;
    let snapshot_id = match &options.snapshot_id {
        Some(id) => Some(id.clone()),
        None => crate::project::load_state(&paths)?.current_checkpoint,
    };
    let project_kind = options.project_kind.clone().or_else(|| {
        crate::project_root::resolve_project_root(root)
            .ok()
            .map(|r| r.kind.as_str().to_string())
    });
    add_media_file_with_options(
        root,
        image,
        MediaKind::Screenshot,
        &AddMediaOptions {
            label: options.label.clone(),
            related_snapshot_id: snapshot_id,
            project_kind,
        },
    )
}

#[derive(Debug, Clone, Default)]
pub struct AddScreenshotOptions {
    pub label: Option<String>,
    /// When omitted, attaches to the current checkpoint tip if one exists.
    pub snapshot_id: Option<SnapshotId>,
    pub project_kind: Option<String>,
}

/// List screenshot attachments (optional visual memory).
pub fn list_screenshots(root: &Path) -> Result<Vec<ProjectMedia>> {
    Ok(list_media(root)?
        .into_iter()
        .filter(|m| m.kind == MediaKind::Screenshot)
        .collect())
}

/// Screenshots linked to a specific snapshot id.
pub fn screenshots_for_snapshot(
    root: &Path,
    snapshot_id: &SnapshotId,
) -> Result<Vec<ProjectMedia>> {
    Ok(list_screenshots(root)?
        .into_iter()
        .filter(|m| m.related_snapshot_id.as_ref() == Some(snapshot_id))
        .collect())
}

/// Report whether a screenshot object's bytes are available (does not affect musical verification).
pub fn screenshot_object_status(
    root: &Path,
    media_id_prefix: &str,
) -> Result<(ProjectMedia, crate::storage::Availability)> {
    let (item, _) = resolve_media_object(root, media_id_prefix)?;
    if item.kind != MediaKind::Screenshot {
        return Err(Error::new(
            ErrorKind::InvalidProject,
            "media item is not a screenshot",
        ));
    }
    let paths = require_initialized(root)?;
    let store = paths.open_local_store()?;
    let availability = store.verify(&item.object_id)?;
    Ok((item, availability))
}

pub fn list_media(root: &Path) -> Result<Vec<ProjectMedia>> {
    let paths = require_initialized(root)?;
    Ok(load_or_default_media(&paths)?.items)
}

pub fn resolve_media_object(root: &Path, media_id_prefix: &str) -> Result<(ProjectMedia, bool)> {
    let paths = require_initialized(root)?;
    let doc = load_or_default_media(&paths)?;
    let item = doc
        .items
        .into_iter()
        .find(|m| {
            m.id.to_string() == media_id_prefix || m.id.to_string().starts_with(media_id_prefix)
        })
        .ok_or_else(|| {
            Error::new(
                ErrorKind::NotFound,
                format!("media item not found: {media_id_prefix}"),
            )
        })?;
    let store = paths.open_local_store()?;
    let present = store.has(&item.object_id)?;
    Ok((item, present))
}

/// Export media object bytes to a temporary file for playback/viewing.
///
/// The caller owns cleanup of the returned path. Does not modify source audio.
pub fn materialize_media_to_temp(root: &Path, media_id_prefix: &str) -> Result<std::path::PathBuf> {
    use std::io::Write;
    let (item, present) = resolve_media_object(root, media_id_prefix)?;
    if !present {
        return Err(Error::new(
            ErrorKind::NotFound,
            format!("media object {} missing from object store", item.object_id),
        ));
    }
    let paths = require_initialized(root)?;
    let store = paths.open_local_store()?;
    let mut reader = store.get(&item.object_id)?;
    let ext = match item.kind {
        MediaKind::AudioPreview => match item.mime_type.as_str() {
            "audio/wav" => "wav",
            "audio/flac" => "flac",
            "audio/mpeg" => "mp3",
            _ => "bin",
        },
        MediaKind::ArrangementImage | MediaKind::Artwork | MediaKind::Screenshot => {
            match item.mime_type.as_str() {
                "image/png" => "png",
                "image/jpeg" => "jpg",
                "image/webp" => "webp",
                _ => "img",
            }
        }
        _ => "bin",
    };
    let tmp = std::env::temp_dir().join(format!(
        "versione-media-{}-{}.{}",
        &item.id.to_string()[..8],
        &item.object_id.to_string()[..8],
        ext
    ));
    let mut out = std::fs::File::create(&tmp)
        .map_err(|e| Error::io("failed to create temp media file", &tmp, e))?;
    std::io::copy(&mut reader, &mut out)
        .map_err(|e| Error::new(ErrorKind::Io, "failed writing temp media").with_source(e))?;
    out.flush()
        .map_err(|e| Error::io("failed flushing temp media", &tmp, e))?;
    Ok(tmp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{init_project, InitOptions};
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn audio_preview_uses_object_store_not_git_bytes() {
        let dir = tempdir().unwrap();
        init_project(dir.path(), &InitOptions::default()).unwrap();
        let wav = dir.path().join("preview.wav");
        fs::write(&wav, b"RIFF....WAVEfmt ").unwrap();
        let media = add_media_file(
            dir.path(),
            &wav,
            MediaKind::AudioPreview,
            Some("loop".into()),
        )
        .unwrap();
        assert_eq!(media.kind, MediaKind::AudioPreview);
        let store = ProjectPaths::from_root(dir.path())
            .open_local_store()
            .unwrap();
        assert!(store.has(&media.object_id).unwrap());
        // media.toml is metadata; objects dir holds bytes.
        assert!(dir.path().join(".versione/media.toml").is_file());
        let objects = dir.path().join(".versione/objects");
        assert!(objects.is_dir());
    }

    #[test]
    fn missing_media_object_reported_safely() {
        let dir = tempdir().unwrap();
        init_project(dir.path(), &InitOptions::default()).unwrap();
        let img = dir.path().join("arr.png");
        fs::write(&img, b"\x89PNG").unwrap();
        let media = add_media_file(dir.path(), &img, MediaKind::ArrangementImage, None).unwrap();
        // Simulate missing object by removing store contents after record exists.
        let store_root = dir.path().join(".versione/objects");
        let _ = fs::remove_dir_all(&store_root);
        fs::create_dir_all(&store_root).unwrap();
        let (item, present) = resolve_media_object(dir.path(), &media.id.to_string()).unwrap();
        assert_eq!(item.id, media.id);
        assert!(!present);
    }

    #[test]
    fn screenshot_imported_and_deduped_by_content() {
        let dir = tempdir().unwrap();
        init_project(dir.path(), &InitOptions::default()).unwrap();
        // Tiny synthetic PNG header bytes (not a real image decode).
        let png = dir.path().join("shot.png");
        fs::write(&png, b"\x89PNG\r\n\x1a\nSHOT").unwrap();
        let a = add_screenshot(dir.path(), &png, &AddScreenshotOptions::default()).unwrap();
        assert_eq!(a.kind, MediaKind::Screenshot);
        assert_eq!(a.source_filename.as_deref(), Some("shot.png"));
        assert!(!a.source_filename.as_deref().unwrap_or("").contains('\\'));
        let store = ProjectPaths::from_root(dir.path())
            .open_local_store()
            .unwrap();
        assert!(store.has(&a.object_id).unwrap());
        assert_eq!(
            store.verify(&a.object_id).unwrap(),
            crate::storage::Availability::Available
        );

        let png2 = dir.path().join("copy.png");
        fs::write(&png2, b"\x89PNG\r\n\x1a\nSHOT").unwrap();
        let b = add_screenshot(dir.path(), &png2, &AddScreenshotOptions::default()).unwrap();
        assert_eq!(a.object_id, b.object_id, "identical bytes must dedupe");
        assert_ne!(a.id, b.id, "metadata rows remain distinct");
        assert_eq!(list_screenshots(dir.path()).unwrap().len(), 2);
    }

    #[test]
    fn screenshots_associate_with_snapshots_independently() {
        use crate::checkpoint::create_checkpoint;

        let dir = tempdir().unwrap();
        init_project(dir.path(), &InitOptions::default()).unwrap();
        fs::write(dir.path().join("track.txt"), b"v1").unwrap();
        let cp1 = match create_checkpoint(dir.path()).unwrap() {
            crate::checkpoint::CheckpointOutcome::Created(r) => r,
            other => panic!("expected created checkpoint, got {other:?}"),
        };
        let content_before = cp1.content_id.clone();

        let shot1 = dir.path().join("s1.png");
        fs::write(&shot1, b"\x89PNG-one").unwrap();
        let m1 = add_screenshot(
            dir.path(),
            &shot1,
            &AddScreenshotOptions {
                label: Some("V-ish".into()),
                snapshot_id: Some(cp1.snapshot_id.clone()),
                project_kind: Some("generic".into()),
            },
        )
        .unwrap();
        assert_eq!(m1.related_snapshot_id.as_ref(), Some(&cp1.snapshot_id));

        fs::write(dir.path().join("track.txt"), b"v2").unwrap();
        let cp2 = match create_checkpoint(dir.path()).unwrap() {
            crate::checkpoint::CheckpointOutcome::Created(r) => r,
            other => panic!("expected created checkpoint, got {other:?}"),
        };
        let shot2 = dir.path().join("s2.png");
        fs::write(&shot2, b"\x89PNG-two").unwrap();
        let m2 = add_screenshot(
            dir.path(),
            &shot2,
            &AddScreenshotOptions {
                snapshot_id: Some(cp2.snapshot_id.clone()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_ne!(m1.object_id, m2.object_id);
        assert_eq!(
            screenshots_for_snapshot(dir.path(), &cp1.snapshot_id)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            screenshots_for_snapshot(dir.path(), &cp2.snapshot_id)
                .unwrap()
                .len(),
            1
        );

        // Musical content_id changes with project files, not because of screenshots.
        assert_ne!(content_before, cp2.content_id);
        // Attaching another screenshot must not rewrite the prior checkpoint tip content.
        let shot3 = dir.path().join("s3.png");
        fs::write(&shot3, b"\x89PNG-three").unwrap();
        add_screenshot(
            dir.path(),
            &shot3,
            &AddScreenshotOptions {
                snapshot_id: Some(cp2.snapshot_id.clone()),
                ..Default::default()
            },
        )
        .unwrap();
        let tip = crate::project::load_state(&ProjectPaths::from_root(dir.path()))
            .unwrap()
            .current_checkpoint
            .unwrap();
        let manifest = crate::manifest::SnapshotManifest::load(
            &ProjectPaths::from_root(dir.path()).manifest_path(&tip),
        )
        .unwrap();
        assert_eq!(manifest.content_id, cp2.content_id);
    }

    #[test]
    fn corrupt_screenshot_does_not_block_musical_verify() {
        use crate::verify::verify_working_tree;

        let dir = tempdir().unwrap();
        init_project(dir.path(), &InitOptions::default()).unwrap();
        fs::write(dir.path().join("song.txt"), b"notes").unwrap();
        let png = dir.path().join("s.png");
        fs::write(&png, b"\x89PNG-x").unwrap();
        let shot = add_screenshot(dir.path(), &png, &AddScreenshotOptions::default()).unwrap();

        let oid = shot.object_id.to_string();
        let path = dir
            .path()
            .join(".versione/objects")
            .join(&oid[..2])
            .join(&oid[2..]);
        assert!(path.is_file());
        fs::write(&path, b"CORRUPT").unwrap();
        let (_item, availability) =
            screenshot_object_status(dir.path(), &shot.id.to_string()).unwrap();
        assert_eq!(availability, crate::storage::Availability::Corrupt);

        let report = verify_working_tree(dir.path()).unwrap();
        assert!(report.ok());
    }

    #[test]
    fn screenshot_bytes_not_in_git_commit() {
        let dir = tempdir().unwrap();
        init_project(dir.path(), &InitOptions::default()).unwrap();
        let png = dir.path().join("view.png");
        let payload = b"\x89PNG\r\n\x1a\nUNIQUE-SHOT-BYTES-xyz";
        fs::write(&png, payload).unwrap();
        add_screenshot(dir.path(), &png, &AddScreenshotOptions::default()).unwrap();

        let output = std::process::Command::new("git")
            .args(["ls-files", ".versione"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        let tracked = String::from_utf8_lossy(&output.stdout);
        assert!(tracked.contains("media.toml"));
        assert!(!tracked
            .lines()
            .any(|l| l.contains("objects/") || l.ends_with("UNIQUE-SHOT-BYTES-xyz")));

        let grep = std::process::Command::new("git")
            .args([
                "grep",
                "-a",
                "UNIQUE-SHOT-BYTES-xyz",
                "HEAD",
                "--",
                ".versione",
            ])
            .current_dir(dir.path())
            .output()
            .unwrap();
        assert!(
            !grep.status.success() || grep.stdout.is_empty(),
            "screenshot payload must not appear in Git metadata"
        );
    }
}
