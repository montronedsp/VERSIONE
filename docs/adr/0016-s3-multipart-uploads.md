# ADR 0016: Bounded-memory S3 multipart uploads

## Status

Accepted

## Context

Early S3 PUT buffered entire objects for Content-Length and retries. That is unsafe for multi-gigabyte music projects.

## Decision

1. Objects ≤ 8 MiB use a single PUT with a bounded in-memory buffer.
2. Larger objects use multipart upload with 8 MiB parts (maximum RAM ≈ one part).
3. Unknown-length streams spill to a temporary disk file, then use the sized path.
4. Failed multipart uploads are aborted; incomplete remote objects are not treated as published.
5. Publish still requires remote HEAD/`Content-Length` verification before Git push.

## Consequences

- `ObjectStore::put_sized` carries known sizes from local metadata into S3.
- Fake transport tests cover single PUT, multipart, and abort-on-interrupt.
- Optional live MinIO testing remains env-gated and is not required for public CI.
