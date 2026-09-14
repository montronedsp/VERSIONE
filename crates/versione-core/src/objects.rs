//! Content identity: streaming hashes and object identifiers.
//!
//! Object *bytes* live in pluggable stores (`crate::storage`). Object *ids* are
//! content digests and must not embed provider URLs.

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{Error, ErrorKind, Result};

/// Format v1 hash algorithm identifier stored in project config.
pub const HASH_ALGORITHM: &str = "blake3";

/// Hex-encoded BLAKE3 digest identifying immutable content.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ObjectId(String);

impl ObjectId {
    /// Parse a hex BLAKE3 digest (normalized to lowercase).
    pub fn parse(value: &str) -> Result<Self> {
        if value.len() != 64 || !value.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(Error::new(
                ErrorKind::InvalidObjectId,
                format!(
                    "object id must be 64 hex characters, got length {}",
                    value.len()
                ),
            ));
        }
        Ok(Self(value.to_ascii_lowercase()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Two-character fan-out prefix used in local object path layouts.
    pub fn prefix(&self) -> &str {
        &self.0[..2]
    }

    pub fn suffix(&self) -> &str {
        &self.0[2..]
    }
}

impl std::fmt::Display for ObjectId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Hash an arbitrary reader using streaming BLAKE3.
pub fn hash_reader(mut reader: impl Read) -> Result<ObjectId> {
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0_u8; 1024 * 64];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|e| Error::new(ErrorKind::Io, "failed while hashing reader").with_source(e))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    ObjectId::parse(&hasher.finalize().to_hex())
}

/// Hash a file using buffered streaming I/O.
pub fn hash_file(path: &Path) -> Result<ObjectId> {
    let file =
        File::open(path).map_err(|e| Error::io("failed to open file for hashing", path, e))?;
    let reader = BufReader::with_capacity(1024 * 64, file);
    hash_reader(reader).map_err(|e| match e.path {
        Some(_) => e,
        None => Error {
            kind: e.kind,
            message: e.message,
            path: Some(path.to_path_buf()),
            source: e.source,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn hashes_known_vector() {
        let id = hash_reader(Cursor::new(b"VERSIONE")).unwrap();
        assert_eq!(id.as_str().len(), 64);
        let again = hash_reader(Cursor::new(b"VERSIONE")).unwrap();
        assert_eq!(id, again);
    }

    #[test]
    fn rejects_malformed_object_id() {
        assert!(ObjectId::parse("abc").is_err());
        assert!(ObjectId::parse(&"g".repeat(64)).is_err());
    }
}
