//! Local filesystem / NAS / external-drive object store.

use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use crate::error::{Error, ErrorKind, Result};
use crate::objects::{hash_reader, ObjectId};
use crate::storage::{Availability, ObjectMetadata, ObjectStore, StorageLocationId};

/// Content-addressed store on a local path (SSD, external drive, NAS mount).
#[derive(Debug, Clone)]
pub struct LocalObjectStore {
    location_id: StorageLocationId,
    root: PathBuf,
}

impl LocalObjectStore {
    pub fn open(location_id: StorageLocationId, root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        fs::create_dir_all(&root)
            .map_err(|e| Error::io("failed to create local object store root", &root, e))?;
        Ok(Self { location_id, root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn relative_path(id: &ObjectId) -> PathBuf {
        PathBuf::from(id.prefix()).join(id.suffix())
    }

    fn object_path(&self, id: &ObjectId) -> PathBuf {
        self.root.join(Self::relative_path(id))
    }

    fn temp_path(&self, id: &ObjectId) -> PathBuf {
        self.root.join(format!(
            ".tmp-{}-{}.partial",
            id.prefix(),
            &id.as_str()[2..10]
        ))
    }
}

impl ObjectStore for LocalObjectStore {
    fn location_id(&self) -> &StorageLocationId {
        &self.location_id
    }

    fn has(&self, id: &ObjectId) -> Result<bool> {
        Ok(self.object_path(id).is_file())
    }

    fn put(&self, id: &ObjectId, reader: &mut dyn Read) -> Result<ObjectMetadata> {
        if self.has(id)? {
            return self.metadata(id);
        }

        let final_path = self.object_path(id);
        if let Some(parent) = final_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| Error::io("failed to create object fan-out directory", parent, e))?;
        }

        let tmp = self.temp_path(id);
        let mut size = 0_u64;
        let mut hasher = blake3::Hasher::new();

        {
            let file = File::create(&tmp)
                .map_err(|e| Error::io("failed to create temporary object file", &tmp, e))?;
            let mut writer = BufWriter::with_capacity(1024 * 64, file);
            let mut buffer = [0_u8; 1024 * 64];
            loop {
                let read = reader.read(&mut buffer).map_err(|e| {
                    Error::new(ErrorKind::Io, "failed while reading object stream").with_source(e)
                })?;
                if read == 0 {
                    break;
                }
                writer
                    .write_all(&buffer[..read])
                    .map_err(|e| Error::io("failed while writing temporary object", &tmp, e))?;
                hasher.update(&buffer[..read]);
                size += read as u64;
            }
            writer
                .flush()
                .map_err(|e| Error::io("failed to flush temporary object", &tmp, e))?;
        }

        let computed = ObjectId::parse(&hasher.finalize().to_hex())?;
        if &computed != id {
            let _ = fs::remove_file(&tmp);
            return Err(Error::new(
                ErrorKind::Corruption,
                format!("object stream hash {computed} does not match expected id {id}"),
            ));
        }

        // Publish only after hash verification of the staged bytes.
        fs::rename(&tmp, &final_path).map_err(|e| {
            let _ = fs::remove_file(&tmp);
            Error::io("failed to publish object into store", &final_path, e)
        })?;

        Ok(ObjectMetadata {
            object_id: id.clone(),
            size,
        })
    }

    fn get(&self, id: &ObjectId) -> Result<Box<dyn Read + Send>> {
        let path = self.object_path(id);
        let file = File::open(&path)
            .map_err(|e| Error::io("failed to open object for reading", &path, e))?;
        Ok(Box::new(BufReader::with_capacity(1024 * 64, file)))
    }

    fn verify(&self, id: &ObjectId) -> Result<Availability> {
        let path = self.object_path(id);
        if !path.is_file() {
            return Ok(Availability::Missing);
        }
        let file = File::open(&path)
            .map_err(|e| Error::io("failed to open object for verify", &path, e))?;
        let actual = hash_reader(BufReader::with_capacity(1024 * 64, file))?;
        if &actual == id {
            Ok(Availability::Available)
        } else {
            Ok(Availability::Corrupt)
        }
    }

    fn metadata(&self, id: &ObjectId) -> Result<ObjectMetadata> {
        let path = self.object_path(id);
        let meta = fs::metadata(&path)
            .map_err(|e| Error::io("failed to read object metadata", &path, e))?;
        Ok(ObjectMetadata {
            object_id: id.clone(),
            size: meta.len(),
        })
    }

    fn remove(&self, id: &ObjectId) -> Result<()> {
        let path = self.object_path(id);
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(Error::io("failed to remove object", &path, e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objects::hash_reader;
    use crate::storage::ObjectStore;
    use std::io::Cursor;
    use tempfile::tempdir;

    #[test]
    fn put_get_verify_round_trip() {
        let dir = tempdir().unwrap();
        let store = LocalObjectStore::open(
            StorageLocationId::new("studio-local"),
            dir.path().join("objects"),
        )
        .unwrap();

        let bytes = b"large-audio-bytes-for-test";
        let id = hash_reader(Cursor::new(bytes)).unwrap();
        store.put(&id, &mut Cursor::new(bytes)).unwrap();
        assert!(store.has(&id).unwrap());
        assert_eq!(store.verify(&id).unwrap(), Availability::Available);

        let mut got = Vec::new();
        store.get(&id).unwrap().read_to_end(&mut got).unwrap();
        assert_eq!(got, bytes);
    }

    #[test]
    fn identical_content_deduplicates() {
        let dir = tempdir().unwrap();
        let store =
            LocalObjectStore::open(StorageLocationId::new("local"), dir.path().join("o")).unwrap();
        let bytes = b"kick.wav-contents";
        let id = hash_reader(Cursor::new(bytes)).unwrap();
        store.put(&id, &mut Cursor::new(bytes)).unwrap();
        store.put(&id, &mut Cursor::new(bytes)).unwrap();
        assert_eq!(store.metadata(&id).unwrap().size, bytes.len() as u64);
    }

    #[test]
    fn rejects_mismatched_hash_on_put() {
        let dir = tempdir().unwrap();
        let store =
            LocalObjectStore::open(StorageLocationId::new("local"), dir.path().join("o")).unwrap();
        let expected = hash_reader(Cursor::new(b"expected")).unwrap();
        let err = store
            .put(&expected, &mut Cursor::new(b"different"))
            .unwrap_err();
        assert_eq!(err.kind, ErrorKind::Corruption);
        assert!(!store.has(&expected).unwrap());
    }

    #[test]
    fn fanout_path_layout() {
        let id = ObjectId::parse(&("ab".to_string() + &"cd".repeat(31))).unwrap();
        assert_eq!(
            LocalObjectStore::relative_path(&id),
            PathBuf::from("ab").join("cd".repeat(31))
        );
    }
}
