//! S3-compatible object store (AWS S3, R2, MinIO, B2 S3 API, and similar).
//!
//! Integrity model for remote publication:
//! - `ObjectId` (BLAKE3) remains the content identity.
//! - After upload, VERSIONE confirms the object exists via HEAD and that
//!   `Content-Length` matches the expected size from the local object/manifest.
//! - S3 ETag is not treated as an MD5 or as VERSIONE's content digest.
//! - Full remote content re-hash is not performed on every publish (expensive for
//!   multi-gigabyte audio); restore from local continues to re-hash every byte.
//!
//! Upload memory model:
//! - Objects at or below [`SINGLE_PUT_MAX_BYTES`] use one PUT with a bounded buffer.
//! - Larger objects use multipart upload with part buffers of [`MULTIPART_PART_BYTES`].
//! - Unknown-size streams spill to a temporary file on disk (not RAM), then upload.
//! - Failed multipart uploads are aborted so incomplete remote state is not left behind.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::PathBuf;

use chrono::{DateTime, Utc};

use crate::error::{Error, ErrorKind, Result};
use crate::objects::ObjectId;
use crate::storage::s3_auth::{self, S3Credentials};
use crate::storage::{Availability, ObjectMetadata, ObjectStore, StorageLocationId};

const UNSIGNED_PAYLOAD: &str = "UNSIGNED-PAYLOAD";
const MAX_RETRIES: u32 = 3;

/// Objects larger than this use multipart upload.
pub const SINGLE_PUT_MAX_BYTES: u64 = 8 * 1024 * 1024;
/// Part size for multipart uploads (also the maximum in-memory buffer per request).
pub const MULTIPART_PART_BYTES: usize = 8 * 1024 * 1024;

/// Non-secret S3-compatible endpoint configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct S3StoreConfig {
    pub location_id: StorageLocationId,
    pub bucket: String,
    pub region: String,
    pub endpoint: Option<String>,
    pub prefix: String,
}

impl S3StoreConfig {
    pub fn object_key(&self, id: &ObjectId) -> String {
        let prefix = self.prefix.trim_matches('/');
        if prefix.is_empty() {
            format!("{}/{}", id.prefix(), id.suffix())
        } else {
            format!("{}/{}/{}", prefix, id.prefix(), id.suffix())
        }
    }
}

/// HTTP transport abstraction so tests can inject a fake S3 endpoint.
pub trait S3Transport: Send + Sync {
    fn request(&self, req: S3Request) -> Result<S3Response>;
}

#[derive(Debug, Clone)]
pub struct S3Request {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
}

#[derive(Debug, Clone)]
pub struct S3Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl S3Response {
    fn header_value(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    fn body_text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

/// Default transport using `ureq` (sync HTTP).
#[derive(Debug, Default)]
pub struct UreqS3Transport;

impl S3Transport for UreqS3Transport {
    fn request(&self, req: S3Request) -> Result<S3Response> {
        let mut builder = ureq::request(&req.method, &req.url);
        for (k, v) in &req.headers {
            builder = builder.set(k, v);
        }
        let response = match req.body {
            Some(body) => builder.send_bytes(&body),
            None => builder.call(),
        };
        match response {
            Ok(resp) => {
                let status = resp.status();
                let headers = resp
                    .headers_names()
                    .into_iter()
                    .filter_map(|name| resp.header(&name).map(|value| (name, value.to_string())))
                    .collect::<Vec<_>>();
                let mut body = Vec::new();
                resp.into_reader().read_to_end(&mut body).map_err(|e| {
                    Error::new(ErrorKind::Storage, "failed reading S3 response body").with_source(e)
                })?;
                Ok(S3Response {
                    status,
                    headers,
                    body,
                })
            }
            Err(ureq::Error::Status(code, resp)) => {
                let headers = resp
                    .headers_names()
                    .into_iter()
                    .filter_map(|name| resp.header(&name).map(|value| (name, value.to_string())))
                    .collect::<Vec<_>>();
                let mut body = Vec::new();
                let _ = resp.into_reader().read_to_end(&mut body);
                Ok(S3Response {
                    status: code,
                    headers,
                    body,
                })
            }
            Err(e) => {
                Err(Error::new(ErrorKind::Storage, "S3 HTTP transport failure").with_source(e))
            }
        }
    }
}

/// S3-compatible content-addressed object store.
pub struct S3ObjectStore {
    config: S3StoreConfig,
    credentials: S3Credentials,
    transport: Box<dyn S3Transport>,
}

impl S3ObjectStore {
    pub fn new(
        config: S3StoreConfig,
        credentials: S3Credentials,
        transport: Box<dyn S3Transport>,
    ) -> Self {
        Self {
            config,
            credentials,
            transport,
        }
    }

    pub fn from_env(config: S3StoreConfig) -> Result<Self> {
        Ok(Self::new(
            config,
            S3Credentials::from_env()?,
            Box::new(UreqS3Transport),
        ))
    }

    fn host_and_url(&self, key: &str, query: &str) -> Result<(String, String, String)> {
        let encoded_key = key
            .split('/')
            .map(|part| urlencoding::encode(part).into_owned())
            .collect::<Vec<_>>()
            .join("/");
        let query_suffix = if query.is_empty() {
            String::new()
        } else {
            format!("?{query}")
        };
        if let Some(endpoint) = &self.config.endpoint {
            let endpoint = endpoint.trim_end_matches('/');
            let host = endpoint
                .trim_start_matches("https://")
                .trim_start_matches("http://")
                .split('/')
                .next()
                .ok_or_else(|| Error::new(ErrorKind::Storage, "invalid S3 endpoint host"))?
                .to_string();
            let url = format!(
                "{endpoint}/{}/{}{query_suffix}",
                self.config.bucket, encoded_key
            );
            let canonical_uri = format!("/{}/{}", self.config.bucket, encoded_key);
            Ok((host, url, canonical_uri))
        } else {
            let host = format!(
                "{}.s3.{}.amazonaws.com",
                self.config.bucket, self.config.region
            );
            let url = format!("https://{host}/{encoded_key}{query_suffix}");
            let canonical_uri = format!("/{encoded_key}");
            Ok((host, url, canonical_uri))
        }
    }

    fn amz_timestamps() -> (String, String) {
        let now: DateTime<Utc> = Utc::now();
        (
            now.format("%Y%m%dT%H%M%SZ").to_string(),
            now.format("%Y%m%d").to_string(),
        )
    }

    fn signed_headers(
        &self,
        method: &str,
        host: &str,
        canonical_uri: &str,
        canonical_query: &str,
        extra: &[(&str, &str)],
    ) -> Vec<(String, String)> {
        let (amz_date, date_stamp) = Self::amz_timestamps();
        let authorization = s3_auth::sign_request(
            &self.credentials,
            method,
            host,
            canonical_uri,
            canonical_query,
            &self.config.region,
            &amz_date,
            &date_stamp,
            UNSIGNED_PAYLOAD,
            extra,
        );
        let mut headers = vec![
            ("host".into(), host.to_string()),
            ("x-amz-content-sha256".into(), UNSIGNED_PAYLOAD.to_string()),
            ("x-amz-date".into(), amz_date),
            ("authorization".into(), authorization),
        ];
        if let Some(token) = &self.credentials.session_token {
            headers.push(("x-amz-security-token".into(), token.clone()));
        }
        for (k, v) in extra {
            headers.push(((*k).to_string(), (*v).to_string()));
        }
        headers
    }

    fn request_with_retries(
        &self,
        mut build: impl FnMut() -> Result<S3Request>,
    ) -> Result<S3Response> {
        let mut attempt = 0u32;
        loop {
            attempt += 1;
            let req = build()?;
            match self.transport.request(req) {
                Ok(resp)
                    if attempt < MAX_RETRIES && matches!(resp.status, 500 | 502 | 503 | 504) =>
                {
                    continue;
                }
                Ok(resp) => return Ok(resp),
                Err(_) if attempt < MAX_RETRIES => continue,
                Err(err) => return Err(err),
            }
        }
    }

    fn head_object(&self, id: &ObjectId) -> Result<Option<ObjectMetadata>> {
        let key = self.config.object_key(id);
        let (host, url, canonical_uri) = self.host_and_url(&key, "")?;
        let headers = self.signed_headers("HEAD", &host, &canonical_uri, "", &[]);
        let resp = self.request_with_retries(|| {
            Ok(S3Request {
                method: "HEAD".into(),
                url: url.clone(),
                headers: headers.clone(),
                body: None,
            })
        })?;
        match resp.status {
            200 => {
                let size = resp
                    .header_value("content-length")
                    .ok_or_else(|| {
                        Error::new(
                            ErrorKind::Storage,
                            "S3 HEAD response missing Content-Length",
                        )
                    })?
                    .parse::<u64>()
                    .map_err(|e| {
                        Error::new(ErrorKind::Storage, "invalid S3 Content-Length").with_source(e)
                    })?;
                Ok(Some(ObjectMetadata {
                    object_id: id.clone(),
                    size,
                }))
            }
            404 => Ok(None),
            code => Err(Error::new(
                ErrorKind::Storage,
                format!("S3 HEAD failed with HTTP {code}"),
            )),
        }
    }

    fn put_single(&self, id: &ObjectId, bytes: &[u8]) -> Result<ObjectMetadata> {
        let key = self.config.object_key(id);
        let (host, url, canonical_uri) = self.host_and_url(&key, "")?;
        let content_length = bytes.len().to_string();
        let headers = self.signed_headers(
            "PUT",
            &host,
            &canonical_uri,
            "",
            &[("content-length", content_length.as_str())],
        );
        let resp = self.request_with_retries(|| {
            Ok(S3Request {
                method: "PUT".into(),
                url: url.clone(),
                headers: headers.clone(),
                body: Some(bytes.to_vec()),
            })
        })?;
        if !(200..300).contains(&resp.status) {
            return Err(Error::new(
                ErrorKind::Storage,
                format!("S3 PUT failed with HTTP {}", resp.status),
            ));
        }
        self.verify_remote_size(id, bytes.len() as u64)
    }

    fn verify_remote_size(&self, id: &ObjectId, expected: u64) -> Result<ObjectMetadata> {
        match self.head_object(id)? {
            Some(meta) if meta.size == expected => Ok(meta),
            Some(meta) => Err(Error::new(
                ErrorKind::Corruption,
                format!(
                    "remote object {id} size {} does not match uploaded {expected}",
                    meta.size
                ),
            )),
            None => Err(Error::new(
                ErrorKind::Storage,
                format!("remote object {id} missing after successful upload"),
            )),
        }
    }

    fn create_multipart_upload(&self, id: &ObjectId) -> Result<String> {
        let key = self.config.object_key(id);
        let query = "uploads=";
        let (host, url, canonical_uri) = self.host_and_url(&key, query)?;
        let headers = self.signed_headers("POST", &host, &canonical_uri, query, &[]);
        let resp = self.request_with_retries(|| {
            Ok(S3Request {
                method: "POST".into(),
                url: url.clone(),
                headers: headers.clone(),
                body: None,
            })
        })?;
        if !(200..300).contains(&resp.status) {
            return Err(Error::new(
                ErrorKind::Storage,
                format!("S3 CreateMultipartUpload failed with HTTP {}", resp.status),
            ));
        }
        parse_xml_tag(&resp.body_text(), "UploadId").ok_or_else(|| {
            Error::new(
                ErrorKind::Storage,
                "S3 CreateMultipartUpload response missing UploadId",
            )
        })
    }

    fn upload_part(
        &self,
        id: &ObjectId,
        upload_id: &str,
        part_number: u32,
        bytes: &[u8],
    ) -> Result<String> {
        let key = self.config.object_key(id);
        let query = format!(
            "partNumber={part_number}&uploadId={}",
            urlencoding::encode(upload_id)
        );
        let (host, url, canonical_uri) = self.host_and_url(&key, &query)?;
        let content_length = bytes.len().to_string();
        let headers = self.signed_headers(
            "PUT",
            &host,
            &canonical_uri,
            &query,
            &[("content-length", content_length.as_str())],
        );
        let resp = self.request_with_retries(|| {
            Ok(S3Request {
                method: "PUT".into(),
                url: url.clone(),
                headers: headers.clone(),
                body: Some(bytes.to_vec()),
            })
        })?;
        if !(200..300).contains(&resp.status) {
            return Err(Error::new(
                ErrorKind::Storage,
                format!(
                    "S3 UploadPart {part_number} failed with HTTP {}",
                    resp.status
                ),
            ));
        }
        resp.header_value("etag")
            .map(|s| s.trim_matches('"').to_string())
            .or_else(|| Some(format!("part-{part_number}")))
            .ok_or_else(|| Error::new(ErrorKind::Storage, "S3 UploadPart missing ETag"))
    }

    fn complete_multipart_upload(
        &self,
        id: &ObjectId,
        upload_id: &str,
        parts: &[(u32, String)],
    ) -> Result<()> {
        let key = self.config.object_key(id);
        let query = format!("uploadId={}", urlencoding::encode(upload_id));
        let (host, url, canonical_uri) = self.host_and_url(&key, &query)?;
        let mut xml = String::from("<CompleteMultipartUpload>");
        for (number, etag) in parts {
            xml.push_str(&format!(
                "<Part><PartNumber>{number}</PartNumber><ETag>\"{etag}\"</ETag></Part>"
            ));
        }
        xml.push_str("</CompleteMultipartUpload>");
        let body = xml.into_bytes();
        let content_length = body.len().to_string();
        let headers = self.signed_headers(
            "POST",
            &host,
            &canonical_uri,
            &query,
            &[
                ("content-length", content_length.as_str()),
                ("content-type", "application/xml"),
            ],
        );
        let resp = self.request_with_retries(|| {
            Ok(S3Request {
                method: "POST".into(),
                url: url.clone(),
                headers: headers.clone(),
                body: Some(body.clone()),
            })
        })?;
        if !(200..300).contains(&resp.status) {
            return Err(Error::new(
                ErrorKind::Storage,
                format!(
                    "S3 CompleteMultipartUpload failed with HTTP {}",
                    resp.status
                ),
            ));
        }
        Ok(())
    }

    fn abort_multipart_upload(&self, id: &ObjectId, upload_id: &str) -> Result<()> {
        let key = self.config.object_key(id);
        let query = format!("uploadId={}", urlencoding::encode(upload_id));
        let (host, url, canonical_uri) = self.host_and_url(&key, &query)?;
        let headers = self.signed_headers("DELETE", &host, &canonical_uri, &query, &[]);
        let resp = self.transport.request(S3Request {
            method: "DELETE".into(),
            url,
            headers,
            body: None,
        })?;
        if matches!(resp.status, 200 | 204 | 404) {
            return Ok(());
        }
        Err(Error::new(
            ErrorKind::Storage,
            format!("S3 AbortMultipartUpload failed with HTTP {}", resp.status),
        ))
    }

    fn put_multipart_from_reader(
        &self,
        id: &ObjectId,
        expected_size: u64,
        reader: &mut dyn Read,
    ) -> Result<ObjectMetadata> {
        let upload_id = self.create_multipart_upload(id)?;
        let mut part_number = 1u32;
        let mut completed: Vec<(u32, String)> = Vec::new();
        let mut total = 0u64;
        let mut buffer = vec![0u8; MULTIPART_PART_BYTES];

        let result = (|| {
            loop {
                let mut filled = 0usize;
                while filled < buffer.len() {
                    let n = reader.read(&mut buffer[filled..]).map_err(|e| {
                        Error::new(ErrorKind::Io, "failed reading object for multipart upload")
                            .with_source(e)
                    })?;
                    if n == 0 {
                        break;
                    }
                    filled += n;
                }
                if filled == 0 {
                    break;
                }
                total += filled as u64;
                let etag = self.upload_part(id, &upload_id, part_number, &buffer[..filled])?;
                completed.push((part_number, etag));
                part_number += 1;
                if filled < buffer.len() {
                    break;
                }
            }
            if total != expected_size {
                return Err(Error::new(
                    ErrorKind::Corruption,
                    format!(
                        "multipart upload read {total} bytes, expected {expected_size} for {id}"
                    ),
                ));
            }
            if completed.is_empty() {
                // S3 requires at least one part; empty objects use single PUT path.
                return Err(Error::new(
                    ErrorKind::Invariant,
                    "multipart upload produced no parts",
                ));
            }
            self.complete_multipart_upload(id, &upload_id, &completed)?;
            self.verify_remote_size(id, expected_size)
        })();

        if result.is_err() {
            let _ = self.abort_multipart_upload(id, &upload_id);
        }
        result
    }

    /// Upload with a known size using bounded memory (single PUT or multipart).
    pub fn put_with_known_size(
        &self,
        id: &ObjectId,
        expected_size: u64,
        reader: &mut dyn Read,
    ) -> Result<ObjectMetadata> {
        if let Some(existing) = self.head_object(id)? {
            if existing.size == expected_size {
                return Ok(existing);
            }
            return Err(Error::new(
                ErrorKind::Corruption,
                format!(
                    "remote object {id} exists with size {}, expected {expected_size}",
                    existing.size
                ),
            ));
        }

        if expected_size <= SINGLE_PUT_MAX_BYTES {
            let mut bytes = Vec::with_capacity(expected_size as usize);
            reader
                .take(expected_size)
                .read_to_end(&mut bytes)
                .map_err(|e| {
                    Error::new(ErrorKind::Io, "failed reading object for S3 upload").with_source(e)
                })?;
            if bytes.len() as u64 != expected_size {
                return Err(Error::new(
                    ErrorKind::Corruption,
                    format!(
                        "object stream ended early: got {} bytes, expected {expected_size}",
                        bytes.len()
                    ),
                ));
            }
            let computed = crate::objects::hash_reader(std::io::Cursor::new(&bytes))?;
            if &computed != id {
                return Err(Error::new(
                    ErrorKind::Corruption,
                    format!("object stream hash {computed} does not match expected id {id}"),
                ));
            }
            return self.put_single(id, &bytes);
        }

        // Multipart: stream parts with bounded buffers. Identity was verified locally
        // before publish; we still enforce exact byte count + remote Content-Length.
        self.put_multipart_from_reader(id, expected_size, reader)
    }

    fn spill_to_tempfile(reader: &mut dyn Read) -> Result<(PathBuf, u64, ObjectId)> {
        let mut hasher = blake3::Hasher::new();
        let tmp = std::env::temp_dir().join(format!(
            "versione-s3-upload-{}.partial",
            uuid::Uuid::new_v4()
        ));
        let mut file = File::create(&tmp)
            .map_err(|e| Error::io("failed to create temp upload spill file", &tmp, e))?;
        let mut buf = [0u8; 64 * 1024];
        let mut size = 0u64;
        loop {
            let n = reader.read(&mut buf).map_err(|e| {
                Error::new(ErrorKind::Io, "failed reading object stream for spill").with_source(e)
            })?;
            if n == 0 {
                break;
            }
            file.write_all(&buf[..n])
                .map_err(|e| Error::io("failed writing spill file", &tmp, e))?;
            hasher.update(&buf[..n]);
            size += n as u64;
        }
        file.flush()
            .map_err(|e| Error::io("failed flushing spill file", &tmp, e))?;
        let id = ObjectId::parse(&hasher.finalize().to_hex())?;
        Ok((tmp, size, id))
    }
}

fn parse_xml_tag(xml: &str, tag: &str) -> Option<String> {
    let start = format!("<{tag}>");
    let end = format!("</{tag}>");
    let begin = xml.find(&start)? + start.len();
    let finish = xml[begin..].find(&end)? + begin;
    Some(xml[begin..finish].to_string())
}

impl ObjectStore for S3ObjectStore {
    fn location_id(&self) -> &StorageLocationId {
        &self.config.location_id
    }

    fn has(&self, id: &ObjectId) -> Result<bool> {
        Ok(self.head_object(id)?.is_some())
    }

    fn put(&self, id: &ObjectId, reader: &mut dyn Read) -> Result<ObjectMetadata> {
        if let Some(existing) = self.head_object(id)? {
            return Ok(existing);
        }
        // Unknown-length stream: spill to disk (bounded RAM), then sized upload.
        let (tmp, size, computed) = Self::spill_to_tempfile(reader)?;
        let cleanup = |path: &PathBuf| {
            let _ = fs::remove_file(path);
        };
        if &computed != id {
            cleanup(&tmp);
            return Err(Error::new(
                ErrorKind::Corruption,
                format!("object stream hash {computed} does not match expected id {id}"),
            ));
        }
        let mut file = match File::open(&tmp) {
            Ok(f) => f,
            Err(e) => {
                cleanup(&tmp);
                return Err(Error::io("failed to reopen spill file for upload", &tmp, e));
            }
        };
        let result = self.put_with_known_size(id, size, &mut file);
        cleanup(&tmp);
        result
    }

    fn put_sized(&self, id: &ObjectId, size: u64, reader: &mut dyn Read) -> Result<ObjectMetadata> {
        self.put_with_known_size(id, size, reader)
    }

    fn get(&self, id: &ObjectId) -> Result<Box<dyn Read + Send>> {
        let key = self.config.object_key(id);
        let (host, url, canonical_uri) = self.host_and_url(&key, "")?;
        let headers = self.signed_headers("GET", &host, &canonical_uri, "", &[]);
        let resp = self.request_with_retries(|| {
            Ok(S3Request {
                method: "GET".into(),
                url: url.clone(),
                headers: headers.clone(),
                body: None,
            })
        })?;
        if resp.status == 404 {
            return Err(Error::new(
                ErrorKind::NotFound,
                format!("remote object {id} not found"),
            ));
        }
        if !(200..300).contains(&resp.status) {
            return Err(Error::new(
                ErrorKind::Storage,
                format!("S3 GET failed with HTTP {}", resp.status),
            ));
        }
        Ok(Box::new(std::io::Cursor::new(resp.body)))
    }

    fn verify(&self, id: &ObjectId) -> Result<Availability> {
        match self.head_object(id)? {
            Some(_) => Ok(Availability::Available),
            None => Ok(Availability::Missing),
        }
    }

    fn metadata(&self, id: &ObjectId) -> Result<ObjectMetadata> {
        self.head_object(id)?
            .ok_or_else(|| Error::new(ErrorKind::NotFound, format!("remote object {id} not found")))
    }

    fn remove(&self, id: &ObjectId) -> Result<()> {
        let _ = id;
        Err(Error::new(
            ErrorKind::Invariant,
            "remote object deletion is not implemented; garbage collection is deferred",
        ))
    }
}

/// Replicate one object from local store to remote with size verification.
pub fn replicate_object(
    local: &dyn ObjectStore,
    remote: &dyn ObjectStore,
    id: &ObjectId,
    expected_size: u64,
) -> Result<ObjectMetadata> {
    if let Ok(meta) = remote.metadata(id) {
        if meta.size == expected_size {
            return Ok(meta);
        }
        return Err(Error::new(
            ErrorKind::Corruption,
            format!(
                "remote object {id} has size {}, expected {expected_size}",
                meta.size
            ),
        ));
    }
    let local_meta = local.metadata(id)?;
    if local_meta.size != expected_size {
        return Err(Error::new(
            ErrorKind::Corruption,
            format!(
                "local object {id} has size {}, expected {expected_size}",
                local_meta.size
            ),
        ));
    }
    if local.verify(id)? != Availability::Available {
        return Err(Error::new(
            ErrorKind::Corruption,
            format!("local object {id} failed verification before upload"),
        ));
    }
    let mut reader = local.get(id)?;
    remote.put_sized(id, expected_size, &mut reader)?;
    let verified = remote.metadata(id)?;
    if verified.size != expected_size {
        return Err(Error::new(
            ErrorKind::Corruption,
            format!(
                "remote object {id} size {} after upload, expected {expected_size}",
                verified.size
            ),
        ));
    }
    Ok(verified)
}

#[cfg(test)]
fn parse_xml_tag_pub(xml: &str, tag: &str) -> Option<String> {
    parse_xml_tag(xml, tag)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objects::hash_reader;
    use crate::storage::s3_auth::S3Credentials;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct FakeS3 {
        objects: Mutex<std::collections::HashMap<String, Vec<u8>>>,
        multipart: Mutex<std::collections::HashMap<String, MultipartState>>,
        fail_next_part: AtomicBool,
        parts_seen: AtomicUsize,
        aborted: AtomicUsize,
    }

    struct MultipartState {
        key: String,
        parts: std::collections::HashMap<u32, Vec<u8>>,
    }

    impl FakeS3 {
        fn key_from_url(url: &str) -> String {
            // http://endpoint/bucket/prefix/aa/bb...
            let without_query = url.split('?').next().unwrap_or(url);
            let parts: Vec<_> = without_query.split('/').collect();
            // find bucket then rest is key
            if parts.len() >= 5 {
                parts[4..].join("/")
            } else {
                without_query.to_string()
            }
        }
    }

    impl S3Transport for FakeS3 {
        fn request(&self, req: S3Request) -> Result<S3Response> {
            let url = req.url.clone();
            let query = url.split('?').nth(1).unwrap_or("");
            let key = Self::key_from_url(&url);

            if req.method == "HEAD" {
                let objects = self.objects.lock().unwrap();
                return if let Some(bytes) = objects.get(&key) {
                    Ok(S3Response {
                        status: 200,
                        headers: vec![("content-length".into(), bytes.len().to_string())],
                        body: Vec::new(),
                    })
                } else {
                    Ok(S3Response {
                        status: 404,
                        headers: Vec::new(),
                        body: Vec::new(),
                    })
                };
            }

            if req.method == "POST" && query.starts_with("uploads") {
                let upload_id = format!("upload-{}", uuid::Uuid::new_v4());
                self.multipart.lock().unwrap().insert(
                    upload_id.clone(),
                    MultipartState {
                        key: key.clone(),
                        parts: Default::default(),
                    },
                );
                let body = format!("<InitiateMultipartUploadResult><UploadId>{upload_id}</UploadId></InitiateMultipartUploadResult>");
                return Ok(S3Response {
                    status: 200,
                    headers: Vec::new(),
                    body: body.into_bytes(),
                });
            }

            if req.method == "PUT" && query.contains("partNumber=") {
                self.parts_seen.fetch_add(1, Ordering::SeqCst);
                if self.fail_next_part.swap(false, Ordering::SeqCst) {
                    return Ok(S3Response {
                        status: 500,
                        headers: Vec::new(),
                        body: b"fail".to_vec(),
                    });
                }
                let part_number: u32 = query
                    .split('&')
                    .find_map(|p| p.strip_prefix("partNumber="))
                    .unwrap()
                    .parse()
                    .unwrap();
                let upload_id = query
                    .split('&')
                    .find_map(|p| p.strip_prefix("uploadId="))
                    .unwrap()
                    .to_string();
                let upload_id = urlencoding::decode(&upload_id).unwrap().into_owned();
                let mut mp = self.multipart.lock().unwrap();
                let state = mp.get_mut(&upload_id).unwrap();
                state
                    .parts
                    .insert(part_number, req.body.clone().unwrap_or_default());
                return Ok(S3Response {
                    status: 200,
                    headers: vec![("etag".into(), format!("\"etag-{part_number}\""))],
                    body: Vec::new(),
                });
            }

            if req.method == "POST" && query.starts_with("uploadId=") {
                let upload_id = urlencoding::decode(query.trim_start_matches("uploadId="))
                    .unwrap()
                    .into_owned();
                let mut mp = self.multipart.lock().unwrap();
                let state = mp.remove(&upload_id).unwrap();
                let mut numbers: Vec<_> = state.parts.keys().copied().collect();
                numbers.sort_unstable();
                let mut bytes = Vec::new();
                for n in numbers {
                    bytes.extend_from_slice(&state.parts[&n]);
                }
                self.objects.lock().unwrap().insert(state.key, bytes);
                return Ok(S3Response {
                    status: 200,
                    headers: Vec::new(),
                    body: b"<CompleteMultipartUploadResult/>".to_vec(),
                });
            }

            if req.method == "DELETE" && query.starts_with("uploadId=") {
                let upload_id = urlencoding::decode(query.trim_start_matches("uploadId="))
                    .unwrap()
                    .into_owned();
                self.multipart.lock().unwrap().remove(&upload_id);
                self.aborted.fetch_add(1, Ordering::SeqCst);
                return Ok(S3Response {
                    status: 204,
                    headers: Vec::new(),
                    body: Vec::new(),
                });
            }

            if req.method == "PUT" {
                let body = req.body.unwrap_or_default();
                self.objects.lock().unwrap().insert(key, body);
                return Ok(S3Response {
                    status: 200,
                    headers: Vec::new(),
                    body: Vec::new(),
                });
            }

            Ok(S3Response {
                status: 404,
                headers: Vec::new(),
                body: Vec::new(),
            })
        }
    }

    fn test_store(transport: Arc<FakeS3>) -> S3ObjectStore {
        // Leak transport into 'static Box via clone wrapper
        struct ArcTransport(Arc<FakeS3>);
        impl S3Transport for ArcTransport {
            fn request(&self, req: S3Request) -> Result<S3Response> {
                self.0.request(req)
            }
        }
        S3ObjectStore::new(
            S3StoreConfig {
                location_id: StorageLocationId::new("remote"),
                bucket: "bucket".into(),
                region: "auto".into(),
                endpoint: Some("http://127.0.0.1:9000".into()),
                prefix: "versione".into(),
            },
            S3Credentials {
                access_key: "test".into(),
                secret_key: "test".into(),
                session_token: None,
            },
            Box::new(ArcTransport(transport)),
        )
    }

    #[test]
    fn small_object_uses_single_put() {
        let fake = Arc::new(FakeS3::default());
        let store = test_store(fake.clone());
        let data = vec![7u8; 1024];
        let id = hash_reader(std::io::Cursor::new(&data)).unwrap();
        store
            .put_with_known_size(&id, data.len() as u64, &mut data.as_slice())
            .unwrap();
        assert_eq!(fake.parts_seen.load(Ordering::SeqCst), 0);
        assert_eq!(store.metadata(&id).unwrap().size, 1024);
    }

    #[test]
    fn large_object_uses_multipart_with_bounded_parts() {
        let fake = Arc::new(FakeS3::default());
        let store = test_store(fake.clone());
        let size = SINGLE_PUT_MAX_BYTES as usize + MULTIPART_PART_BYTES + 100;
        let data = vec![9u8; size];
        let id = hash_reader(std::io::Cursor::new(&data)).unwrap();
        store
            .put_with_known_size(&id, size as u64, &mut data.as_slice())
            .unwrap();
        assert!(fake.parts_seen.load(Ordering::SeqCst) >= 2);
        assert_eq!(store.metadata(&id).unwrap().size, size as u64);
        assert!(fake.multipart.lock().unwrap().is_empty());
    }

    #[test]
    fn interrupted_multipart_aborts_and_leaves_no_object() {
        let fake = Arc::new(FakeS3::default());
        fake.fail_next_part.store(true, Ordering::SeqCst);
        // Exhaust retries on first part by failing repeatedly.
        // request_with_retries retries 500s — set fail only once would succeed on retry.
        // Use a custom transport that always fails parts:
        struct AlwaysFailPart(Arc<FakeS3>);
        impl S3Transport for AlwaysFailPart {
            fn request(&self, req: S3Request) -> Result<S3Response> {
                if req.method == "PUT" && req.url.contains("partNumber=") {
                    return Ok(S3Response {
                        status: 500,
                        headers: Vec::new(),
                        body: b"fail".to_vec(),
                    });
                }
                self.0.request(req)
            }
        }
        let store = S3ObjectStore::new(
            S3StoreConfig {
                location_id: StorageLocationId::new("remote"),
                bucket: "bucket".into(),
                region: "auto".into(),
                endpoint: Some("http://127.0.0.1:9000".into()),
                prefix: "versione".into(),
            },
            S3Credentials {
                access_key: "test".into(),
                secret_key: "test".into(),
                session_token: None,
            },
            Box::new(AlwaysFailPart(fake.clone())),
        );
        let size = SINGLE_PUT_MAX_BYTES as usize + 10;
        let data = vec![1u8; size];
        let id = hash_reader(std::io::Cursor::new(&data)).unwrap();
        let err = store
            .put_with_known_size(&id, size as u64, &mut data.as_slice())
            .unwrap_err();
        assert_eq!(err.kind, ErrorKind::Storage);
        assert!(fake.aborted.load(Ordering::SeqCst) >= 1);
        assert!(store.head_object(&id).unwrap().is_none());
        assert!(fake.multipart.lock().unwrap().is_empty());
    }

    #[test]
    fn parse_upload_id() {
        let xml = "<UploadId>abc-123</UploadId>";
        assert_eq!(
            parse_xml_tag_pub(xml, "UploadId").as_deref(),
            Some("abc-123")
        );
    }
}
