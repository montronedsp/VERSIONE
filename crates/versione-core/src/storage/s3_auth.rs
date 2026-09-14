//! AWS Signature Version 4 helpers for S3-compatible PUT/HEAD/GET.
//!
//! Credentials are read from the environment and never logged.

use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

use crate::error::{Error, ErrorKind, Result};

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone)]
pub struct S3Credentials {
    pub access_key: String,
    pub secret_key: String,
    pub session_token: Option<String>,
}

impl S3Credentials {
    /// Load credentials from environment variables.
    ///
    /// Prefers `VERSIONE_S3_*`, then falls back to standard `AWS_*` names.
    pub fn from_env() -> Result<Self> {
        let access_key = std::env::var("VERSIONE_S3_ACCESS_KEY_ID")
            .or_else(|_| std::env::var("AWS_ACCESS_KEY_ID"))
            .map_err(|_| {
                Error::new(
                    ErrorKind::Storage,
                    "S3 access key not found; set VERSIONE_S3_ACCESS_KEY_ID or AWS_ACCESS_KEY_ID",
                )
            })?;
        let secret_key = std::env::var("VERSIONE_S3_SECRET_ACCESS_KEY")
            .or_else(|_| std::env::var("AWS_SECRET_ACCESS_KEY"))
            .map_err(|_| {
                Error::new(
                    ErrorKind::Storage,
                    "S3 secret key not found; set VERSIONE_S3_SECRET_ACCESS_KEY or AWS_SECRET_ACCESS_KEY",
                )
            })?;
        let session_token = std::env::var("VERSIONE_S3_SESSION_TOKEN")
            .or_else(|_| std::env::var("AWS_SESSION_TOKEN"))
            .ok();
        if access_key.trim().is_empty() || secret_key.trim().is_empty() {
            return Err(Error::new(
                ErrorKind::Storage,
                "S3 credentials environment variables must not be empty",
            ));
        }
        Ok(Self {
            access_key,
            secret_key,
            session_token,
        })
    }
}

pub fn sha256_hex(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

fn signing_key(secret: &str, date_stamp: &str, region: &str, service: &str) -> Vec<u8> {
    let k_date = hmac_sha256(format!("AWS4{secret}").as_bytes(), date_stamp.as_bytes());
    let k_region = hmac_sha256(&k_date, region.as_bytes());
    let k_service = hmac_sha256(&k_region, service.as_bytes());
    hmac_sha256(&k_service, b"aws4_request")
}

/// Build an Authorization header for an S3 request using UNSIGNED-PAYLOAD.
#[allow(clippy::too_many_arguments)] // SigV4 signing inputs are intentionally explicit.
pub fn sign_request(
    credentials: &S3Credentials,
    method: &str,
    host: &str,
    canonical_uri: &str,
    canonical_querystring: &str,
    region: &str,
    amz_date: &str,
    date_stamp: &str,
    content_sha256: &str,
    extra_headers: &[(&str, &str)],
) -> String {
    let mut headers: Vec<(String, String)> = vec![
        ("host".into(), host.to_string()),
        ("x-amz-content-sha256".into(), content_sha256.to_string()),
        ("x-amz-date".into(), amz_date.to_string()),
    ];
    if let Some(token) = &credentials.session_token {
        headers.push(("x-amz-security-token".into(), token.clone()));
    }
    for (k, v) in extra_headers {
        headers.push((k.to_ascii_lowercase(), (*v).to_string()));
    }
    headers.sort_by(|a, b| a.0.cmp(&b.0));

    let signed_headers = headers
        .iter()
        .map(|(k, _)| k.as_str())
        .collect::<Vec<_>>()
        .join(";");
    let canonical_headers = headers
        .iter()
        .map(|(k, v)| format!("{k}:{}\n", v.trim()))
        .collect::<String>();

    let canonical_request = format!(
        "{method}\n{canonical_uri}\n{canonical_querystring}\n{canonical_headers}\n{signed_headers}\n{content_sha256}"
    );
    let credential_scope = format!("{date_stamp}/{region}/s3/aws4_request");
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{amz_date}\n{credential_scope}\n{}",
        sha256_hex(canonical_request.as_bytes())
    );
    let signature = hex::encode(hmac_sha256(
        &signing_key(&credentials.secret_key, date_stamp, region, "s3"),
        string_to_sign.as_bytes(),
    ));
    format!(
        "AWS4-HMAC-SHA256 Credential={}/{credential_scope}, SignedHeaders={signed_headers}, Signature={signature}",
        credentials.access_key
    )
}
