//! Pluggable content-addressed object storage.
//!
//! Git tracks lightweight history and manifests. Project file bytes live here.
//! Object identity is independent of where replicas are stored.

mod local;
mod memory;
mod s3;
mod s3_auth;

use std::io::Read;

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::objects::ObjectId;

pub use local::LocalObjectStore;
pub use memory::MemoryObjectStore;
pub use s3::{
    replicate_object, S3ObjectStore, S3StoreConfig, S3Transport, UreqS3Transport,
    MULTIPART_PART_BYTES, SINGLE_PUT_MAX_BYTES,
};
pub use s3_auth::S3Credentials;

/// Stable name for a configured storage location (not a URL, not an ObjectId).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StorageLocationId(String);

impl StorageLocationId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for StorageLocationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Whether an object is present at a location.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Availability {
    Available,
    Missing,
    Corrupt,
    Unknown,
}

/// One replica of an object at a named location.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectLocation {
    pub location_id: StorageLocationId,
    pub availability: Availability,
    pub verified: bool,
}

/// Metadata returned by store backends (no credentials).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectMetadata {
    pub object_id: ObjectId,
    pub size: u64,
}

/// Streaming object store interface.
///
/// Implementations: local disk/NAS, S3-compatible remotes, and in-memory test doubles.
/// Git LFS may wrap this interface optionally — it is never mandatory.
pub trait ObjectStore {
    fn location_id(&self) -> &StorageLocationId;

    fn has(&self, id: &ObjectId) -> Result<bool>;

    fn put(&self, id: &ObjectId, reader: &mut dyn Read) -> Result<ObjectMetadata>;

    /// Upload with a known Content-Length. Default falls back to [`Self::put`].
    /// S3 uses this for bounded-memory single PUT / multipart.
    fn put_sized(&self, id: &ObjectId, size: u64, reader: &mut dyn Read) -> Result<ObjectMetadata> {
        let _ = size;
        self.put(id, reader)
    }

    fn get(&self, id: &ObjectId) -> Result<Box<dyn Read + Send>>;

    fn verify(&self, id: &ObjectId) -> Result<Availability>;

    fn metadata(&self, id: &ObjectId) -> Result<ObjectMetadata>;

    /// Remove is intentionally conservative; callers must ensure another replica exists.
    fn remove(&self, id: &ObjectId) -> Result<()>;
}

/// Kind of storage backend (configuration / discovery).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageBackendKind {
    Local,
    S3Compatible,
    WebDav,
    Rclone,
    GitLfs,
}
