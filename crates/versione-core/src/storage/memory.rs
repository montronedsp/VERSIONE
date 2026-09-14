//! In-memory object store for tests and local simulation of remote backends.

use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::sync::Mutex;

use crate::error::{Error, ErrorKind, Result};
use crate::objects::{hash_reader, ObjectId};
use crate::storage::{Availability, ObjectMetadata, ObjectStore, StorageLocationId};

/// Content-addressed store kept entirely in process memory.
#[derive(Debug)]
pub struct MemoryObjectStore {
    location_id: StorageLocationId,
    objects: Mutex<HashMap<String, Vec<u8>>>,
    /// When set, `put` fails for this object id (used to simulate upload failures).
    fail_put_for: Mutex<Option<ObjectId>>,
    /// When true, `has`/`metadata` pretend objects are missing even if stored.
    hide_existing: Mutex<bool>,
}

impl MemoryObjectStore {
    pub fn new(location_id: StorageLocationId) -> Self {
        Self {
            location_id,
            objects: Mutex::new(HashMap::new()),
            fail_put_for: Mutex::new(None),
            hide_existing: Mutex::new(false),
        }
    }

    pub fn set_fail_put_for(&self, id: Option<ObjectId>) {
        *self.fail_put_for.lock().expect("memory store lock") = id;
    }

    pub fn set_hide_existing(&self, hide: bool) {
        *self.hide_existing.lock().expect("memory store lock") = hide;
    }

    pub fn len(&self) -> usize {
        self.objects.lock().expect("memory store lock").len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl ObjectStore for MemoryObjectStore {
    fn location_id(&self) -> &StorageLocationId {
        &self.location_id
    }

    fn has(&self, id: &ObjectId) -> Result<bool> {
        if *self.hide_existing.lock().expect("memory store lock") {
            return Ok(false);
        }
        Ok(self
            .objects
            .lock()
            .expect("memory store lock")
            .contains_key(id.as_str()))
    }

    fn put(&self, id: &ObjectId, reader: &mut dyn Read) -> Result<ObjectMetadata> {
        if let Some(fail) = self
            .fail_put_for
            .lock()
            .expect("memory store lock")
            .as_ref()
        {
            if fail == id {
                return Err(Error::new(
                    ErrorKind::Storage,
                    format!("simulated remote upload failure for {id}"),
                ));
            }
        }
        if self.has(id)? {
            return self.metadata(id);
        }
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).map_err(|e| {
            Error::new(
                ErrorKind::Io,
                "failed to read object stream into memory store",
            )
            .with_source(e)
        })?;
        let computed = hash_reader(Cursor::new(&bytes))?;
        if &computed != id {
            return Err(Error::new(
                ErrorKind::Corruption,
                format!("object stream hash {computed} does not match expected id {id}"),
            ));
        }
        let size = bytes.len() as u64;
        self.objects
            .lock()
            .expect("memory store lock")
            .insert(id.as_str().to_string(), bytes);
        Ok(ObjectMetadata {
            object_id: id.clone(),
            size,
        })
    }

    fn get(&self, id: &ObjectId) -> Result<Box<dyn Read + Send>> {
        if *self.hide_existing.lock().expect("memory store lock") {
            return Err(
                Error::new(ErrorKind::NotFound, format!("object {id} not found"))
                    .with_path(id.as_str()),
            );
        }
        let objects = self.objects.lock().expect("memory store lock");
        let Some(bytes) = objects.get(id.as_str()) else {
            return Err(
                Error::new(ErrorKind::NotFound, format!("object {id} not found"))
                    .with_path(id.as_str()),
            );
        };
        Ok(Box::new(Cursor::new(bytes.clone())))
    }

    fn verify(&self, id: &ObjectId) -> Result<Availability> {
        if *self.hide_existing.lock().expect("memory store lock") {
            return Ok(Availability::Missing);
        }
        let objects = self.objects.lock().expect("memory store lock");
        let Some(bytes) = objects.get(id.as_str()) else {
            return Ok(Availability::Missing);
        };
        let actual = hash_reader(Cursor::new(bytes))?;
        if &actual == id {
            Ok(Availability::Available)
        } else {
            Ok(Availability::Corrupt)
        }
    }

    fn metadata(&self, id: &ObjectId) -> Result<ObjectMetadata> {
        if *self.hide_existing.lock().expect("memory store lock") {
            return Err(
                Error::new(ErrorKind::NotFound, format!("object {id} not found"))
                    .with_path(id.as_str()),
            );
        }
        let objects = self.objects.lock().expect("memory store lock");
        let Some(bytes) = objects.get(id.as_str()) else {
            return Err(
                Error::new(ErrorKind::NotFound, format!("object {id} not found"))
                    .with_path(id.as_str()),
            );
        };
        Ok(ObjectMetadata {
            object_id: id.clone(),
            size: bytes.len() as u64,
        })
    }

    fn remove(&self, id: &ObjectId) -> Result<()> {
        self.objects
            .lock()
            .expect("memory store lock")
            .remove(id.as_str());
        Ok(())
    }
}
