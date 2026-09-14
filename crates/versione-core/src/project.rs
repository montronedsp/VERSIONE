//! VERSIONE project identity, paths, storage config, and mutable state.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{Error, ErrorKind, Result};
use crate::git::GitRepository;
use crate::objects::HASH_ALGORITHM;
use crate::snapshot::SnapshotId;
use crate::storage::{LocalObjectStore, StorageLocationId};

/// Stable identifier for a VERSIONE-managed music project.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProjectId(Uuid);

impl ProjectId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl Default for ProjectId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for ProjectId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Current on-disk format version for `.versione/` metadata.
pub const FORMAT_VERSION: u32 = 1;

/// Options for initializing a VERSIONE project beside a DAW project tree.
#[derive(Debug, Clone, Default)]
pub struct InitOptions {
    /// When true, succeed if an identical-compatible project already exists.
    pub allow_existing: bool,
}

/// Persisted project configuration (`config.toml`).
///
/// Must never contain cloud credentials or tokens.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectConfig {
    pub format_version: u32,
    pub project_id: ProjectId,
    pub created_utc: DateTime<Utc>,
    pub hash_algorithm: String,
}

impl ProjectConfig {
    pub fn new_v1() -> Self {
        Self {
            format_version: FORMAT_VERSION,
            project_id: ProjectId::new(),
            created_utc: Utc::now(),
            hash_algorithm: HASH_ALGORITHM.to_string(),
        }
    }
}

/// Local object-store configuration (`storage.toml`). No credentials.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StorageConfig {
    pub format_version: u32,
    pub primary_local_id: String,
    /// Object store root relative to `.versione/` (Phase 1 default: `objects`).
    pub primary_local_root: String,
    /// Optional remote object-store configuration (no secrets).
    #[serde(default)]
    pub remote: Option<RemoteStorageConfig>,
}

/// Remote S3-compatible store settings. Credentials come from the environment only.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteStorageConfig {
    /// Backend kind string, currently `s3`.
    pub kind: String,
    pub location_id: String,
    pub bucket: String,
    pub region: String,
    #[serde(default)]
    pub endpoint: Option<String>,
    #[serde(default)]
    pub prefix: String,
}

impl StorageConfig {
    pub fn new_v1_default() -> Self {
        Self {
            format_version: FORMAT_VERSION,
            primary_local_id: "local".into(),
            primary_local_root: "objects".into(),
            remote: None,
        }
    }
}

/// Mutable tip pointers and version counter (`state.toml`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectState {
    pub format_version: u32,
    pub current_checkpoint: Option<SnapshotId>,
    pub current_version: Option<SnapshotId>,
    pub next_version_number: u32,
}

impl ProjectState {
    pub fn new_v1() -> Self {
        Self {
            format_version: FORMAT_VERSION,
            current_checkpoint: None,
            current_version: None,
            next_version_number: 1,
        }
    }
}

/// Named Version record list (`refs/versions.toml`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct VersionIndex {
    pub format_version: u32,
    pub versions: Vec<VersionRecord>,
}

/// One musician-facing Version.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VersionRecord {
    pub number: u32,
    pub display: String,
    pub name: String,
    pub snapshot_id: SnapshotId,
    pub created_utc: DateTime<Utc>,
}

/// Location of a music project root and its `.versione` metadata directory.
#[derive(Debug, Clone)]
pub struct ProjectPaths {
    pub root: PathBuf,
    pub versione_dir: PathBuf,
}

impl ProjectPaths {
    pub fn from_root(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        let versione_dir = root.join(".versione");
        Self { root, versione_dir }
    }

    pub fn config_path(&self) -> PathBuf {
        self.versione_dir.join("config.toml")
    }

    pub fn storage_config_path(&self) -> PathBuf {
        self.versione_dir.join("storage.toml")
    }

    pub fn state_path(&self) -> PathBuf {
        self.versione_dir.join("state.toml")
    }

    pub fn versions_index_path(&self) -> PathBuf {
        self.versione_dir.join("refs").join("versions.toml")
    }

    pub fn publication_index_path(&self) -> PathBuf {
        self.versione_dir.join("refs").join("publication.toml")
    }

    pub fn lock_path(&self) -> PathBuf {
        self.versione_dir.join("lock")
    }

    pub fn manifests_dir(&self) -> PathBuf {
        self.versione_dir.join("manifests")
    }

    pub fn metadata_dir(&self) -> PathBuf {
        self.versione_dir.join("metadata")
    }

    pub fn refs_dir(&self) -> PathBuf {
        self.versione_dir.join("refs")
    }

    pub fn manifest_path(&self, id: &SnapshotId) -> PathBuf {
        self.manifests_dir().join(format!("{id}.toml"))
    }

    pub fn objects_root(&self, storage: &StorageConfig) -> PathBuf {
        self.versione_dir.join(&storage.primary_local_root)
    }

    pub fn open_local_store(&self) -> Result<LocalObjectStore> {
        let storage = load_storage_config(self)?;
        LocalObjectStore::open(
            StorageLocationId::new(storage.primary_local_id.clone()),
            self.objects_root(&storage),
        )
    }
}

/// Read-only status summary for CLI / UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectStatus {
    pub root: PathBuf,
    pub initialized: bool,
    pub config: Option<ProjectConfig>,
}

/// Initialize `.versione/` beside an existing project directory and ensure Git exists.
pub fn init_project(root: &Path, options: &InitOptions) -> Result<ProjectConfig> {
    if !root.exists() {
        return Err(Error::new(ErrorKind::NotFound, "project root does not exist").with_path(root));
    }
    if !root.is_dir() {
        return Err(Error::new(
            ErrorKind::InvalidProject,
            "project root must be a directory",
        )
        .with_path(root));
    }

    let paths = ProjectPaths::from_root(root);
    if paths.config_path().exists() {
        if options.allow_existing {
            return load_config(&paths);
        }
        return Err(Error::new(
            ErrorKind::AlreadyExists,
            "project already initialized with VERSIONE",
        )
        .with_path(paths.config_path()));
    }

    if paths.versione_dir.exists() && !paths.versione_dir.is_dir() {
        return Err(Error::new(
            ErrorKind::InvalidProject,
            ".versione exists but is not a directory",
        )
        .with_path(&paths.versione_dir));
    }

    fs::create_dir_all(&paths.versione_dir).map_err(|e| {
        Error::io(
            "failed to create .versione directory",
            &paths.versione_dir,
            e,
        )
    })?;

    for dir in [
        paths.manifests_dir(),
        paths.metadata_dir(),
        paths.refs_dir(),
    ] {
        fs::create_dir_all(&dir)
            .map_err(|e| Error::io("failed to create VERSIONE subdirectory", &dir, e))?;
    }

    let config = ProjectConfig::new_v1();
    write_toml_atomic(&paths.config_path(), &config)?;

    let storage = StorageConfig::new_v1_default();
    write_toml_atomic(&paths.storage_config_path(), &storage)?;
    fs::create_dir_all(paths.objects_root(&storage)).map_err(|e| {
        Error::io(
            "failed to create local object store directory",
            paths.objects_root(&storage),
            e,
        )
    })?;

    let state = ProjectState::new_v1();
    write_toml_atomic(&paths.state_path(), &state)?;

    let versions = VersionIndex {
        format_version: FORMAT_VERSION,
        versions: Vec::new(),
    };
    write_toml_atomic(&paths.versions_index_path(), &versions)?;

    let git = GitRepository::new(root);
    git.ensure_initialized()?;
    git.commit_versione_metadata("Initialize VERSIONE project")?;

    Ok(config)
}

pub fn project_status(root: &Path) -> Result<ProjectStatus> {
    let paths = ProjectPaths::from_root(root);
    if !paths.config_path().exists() {
        return Ok(ProjectStatus {
            root: paths.root,
            initialized: false,
            config: None,
        });
    }
    let config = load_config(&paths)?;
    Ok(ProjectStatus {
        root: paths.root,
        initialized: true,
        config: Some(config),
    })
}

pub fn load_config(paths: &ProjectPaths) -> Result<ProjectConfig> {
    let config: ProjectConfig = read_toml(&paths.config_path())?;
    if config.format_version != FORMAT_VERSION {
        return Err(Error::new(
            ErrorKind::UnsupportedFormat,
            format!(
                "unsupported format_version {} (this build supports {})",
                config.format_version, FORMAT_VERSION
            ),
        )
        .with_path(paths.config_path()));
    }
    if config.hash_algorithm != HASH_ALGORITHM {
        return Err(Error::new(
            ErrorKind::UnsupportedFormat,
            format!(
                "unsupported hash_algorithm '{}' (this build supports '{}')",
                config.hash_algorithm, HASH_ALGORITHM
            ),
        )
        .with_path(paths.config_path()));
    }
    Ok(config)
}

pub fn load_storage_config(paths: &ProjectPaths) -> Result<StorageConfig> {
    let storage: StorageConfig = read_toml(&paths.storage_config_path())?;
    if storage.format_version != FORMAT_VERSION {
        return Err(Error::new(
            ErrorKind::UnsupportedFormat,
            format!(
                "unsupported storage format_version {}",
                storage.format_version
            ),
        )
        .with_path(paths.storage_config_path()));
    }
    if Path::new(&storage.primary_local_root).is_absolute()
        || storage.primary_local_root.contains("..")
    {
        return Err(Error::new(
            ErrorKind::PathEscape,
            "primary_local_root must be a relative path without '..'",
        )
        .with_path(paths.storage_config_path()));
    }
    Ok(storage)
}

pub fn load_state(paths: &ProjectPaths) -> Result<ProjectState> {
    let state: ProjectState = read_toml(&paths.state_path())?;
    if state.format_version != FORMAT_VERSION {
        return Err(Error::new(
            ErrorKind::UnsupportedFormat,
            format!("unsupported state format_version {}", state.format_version),
        )
        .with_path(paths.state_path()));
    }
    Ok(state)
}

pub fn save_state(paths: &ProjectPaths, state: &ProjectState) -> Result<()> {
    write_toml_atomic(&paths.state_path(), state)
}

pub fn load_version_index(paths: &ProjectPaths) -> Result<VersionIndex> {
    if !paths.versions_index_path().exists() {
        return Ok(VersionIndex {
            format_version: FORMAT_VERSION,
            versions: Vec::new(),
        });
    }
    let index: VersionIndex = read_toml(&paths.versions_index_path())?;
    if index.format_version != FORMAT_VERSION {
        return Err(Error::new(
            ErrorKind::UnsupportedFormat,
            format!(
                "unsupported versions index format_version {}",
                index.format_version
            ),
        )
        .with_path(paths.versions_index_path()));
    }
    Ok(index)
}

pub fn save_version_index(paths: &ProjectPaths, index: &VersionIndex) -> Result<()> {
    if let Some(parent) = paths.versions_index_path().parent() {
        fs::create_dir_all(parent)
            .map_err(|e| Error::io("failed to create refs directory", parent, e))?;
    }
    write_toml_atomic(&paths.versions_index_path(), index)
}

/// Publication / replica tracking for snapshots (cached hints; remote verify wins).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct PublicationIndex {
    pub format_version: u32,
    #[serde(default)]
    pub snapshots: Vec<PublicationRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PublicationRecord {
    pub snapshot_id: SnapshotId,
    pub remote_verified: bool,
    #[serde(default)]
    pub remote_verified_utc: Option<DateTime<Utc>>,
    pub git_pushed: bool,
    #[serde(default)]
    pub git_pushed_utc: Option<DateTime<Utc>>,
    #[serde(default)]
    pub objects_uploaded: u64,
    #[serde(default)]
    pub objects_reused: u64,
}

pub fn load_publication_index(paths: &ProjectPaths) -> Result<PublicationIndex> {
    if !paths.publication_index_path().exists() {
        return Ok(PublicationIndex {
            format_version: FORMAT_VERSION,
            snapshots: Vec::new(),
        });
    }
    let index: PublicationIndex = read_toml(&paths.publication_index_path())?;
    if index.format_version != FORMAT_VERSION {
        return Err(Error::new(
            ErrorKind::UnsupportedFormat,
            format!(
                "unsupported publication index format_version {}",
                index.format_version
            ),
        )
        .with_path(paths.publication_index_path()));
    }
    Ok(index)
}

pub fn save_publication_index(paths: &ProjectPaths, index: &PublicationIndex) -> Result<()> {
    if let Some(parent) = paths.publication_index_path().parent() {
        fs::create_dir_all(parent)
            .map_err(|e| Error::io("failed to create refs directory", parent, e))?;
    }
    write_toml_atomic(&paths.publication_index_path(), index)
}

pub fn upsert_publication_record(paths: &ProjectPaths, record: PublicationRecord) -> Result<()> {
    let mut index = load_publication_index(paths)?;
    if let Some(existing) = index
        .snapshots
        .iter_mut()
        .find(|r| r.snapshot_id == record.snapshot_id)
    {
        *existing = record;
    } else {
        index.snapshots.push(record);
    }
    index.format_version = FORMAT_VERSION;
    save_publication_index(paths, &index)
}

pub fn configure_remote_storage(root: &Path, remote: RemoteStorageConfig) -> Result<StorageConfig> {
    let paths = require_initialized(root)?;
    if remote.kind != "s3" {
        return Err(Error::new(
            ErrorKind::UnsupportedFormat,
            format!(
                "unsupported remote storage kind '{}'; expected 's3'",
                remote.kind
            ),
        ));
    }
    if remote.bucket.trim().is_empty() || remote.region.trim().is_empty() {
        return Err(Error::new(
            ErrorKind::InvalidProject,
            "remote bucket and region are required",
        ));
    }
    let mut storage = load_storage_config(&paths)?;
    storage.remote = Some(remote);
    write_toml_atomic(&paths.storage_config_path(), &storage)?;
    let git = GitRepository::new(root);
    let _ = git.commit_versione_metadata("Configure remote object storage")?;
    Ok(storage)
}

pub fn require_initialized(root: &Path) -> Result<ProjectPaths> {
    let paths = ProjectPaths::from_root(root);
    if !paths.config_path().exists() {
        return Err(Error::new(
            ErrorKind::InvalidProject,
            "directory is not a VERSIONE project; run `versione init` first",
        )
        .with_path(root));
    }
    let _ = load_config(&paths)?;
    Ok(paths)
}

pub(crate) fn read_toml<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    let text =
        fs::read_to_string(path).map_err(|e| Error::io("failed to read TOML file", path, e))?;
    toml::from_str(&text).map_err(|e| {
        Error::new(
            ErrorKind::Corruption,
            format!("invalid TOML in {}: {e}", path.display()),
        )
        .with_path(path)
    })
}

pub(crate) fn write_toml_atomic<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let rendered = toml::to_string_pretty(value).map_err(|e| {
        Error::new(
            ErrorKind::Invariant,
            format!("failed to serialize TOML for {}: {e}", path.display()),
        )
    })?;
    let tmp = path.with_extension("tmp");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| Error::io("failed to create parent directory", parent, e))?;
    }
    fs::write(&tmp, rendered.as_bytes())
        .map_err(|e| Error::io("failed to write temporary TOML", &tmp, e))?;
    fs::rename(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        Error::io("failed to publish TOML", path, e)
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn init_creates_format_v1() {
        let dir = tempdir().unwrap();
        let config = init_project(dir.path(), &InitOptions::default()).unwrap();
        assert_eq!(config.format_version, 1);
        assert_eq!(config.hash_algorithm, "blake3");
        assert!(dir.path().join(".versione/config.toml").is_file());
        assert!(dir.path().join(".versione/storage.toml").is_file());
        assert!(dir.path().join(".versione/objects").is_dir());
        assert!(dir.path().join(".git").is_dir());
    }

    #[test]
    fn init_refuses_double_init() {
        let dir = tempdir().unwrap();
        init_project(dir.path(), &InitOptions::default()).unwrap();
        let err = init_project(dir.path(), &InitOptions::default()).unwrap_err();
        assert_eq!(err.kind, ErrorKind::AlreadyExists);
    }

    #[test]
    fn status_reports_uninitialized() {
        let dir = tempdir().unwrap();
        let status = project_status(dir.path()).unwrap();
        assert!(!status.initialized);
    }
}
