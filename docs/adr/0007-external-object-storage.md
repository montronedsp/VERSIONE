# ADR 0007: External object storage

- Status: Accepted
- Date: 2026-09-13

## Context

Music projects contain assets too large for sane GitHub/Git usage. Users need local disks, NAS, and S3-compatible clouds without locking into one vendor or mandating Git LFS.

## Decision

- Introduce `ObjectStore` with streaming put/get/has/verify.
- Ship **LocalObjectStore** first.
- Design for **S3-compatible** remotes next (provider-agnostic).
- Leave extension points for WebDAV, rclone, Git LFS.
- Model multiple replicas per `ObjectId` via location ids.
- Keep credentials out of Git-tracked project metadata.

## Consequences

Top-level `storage/` documents backends. Implementations live in `versione-core` (and future crates) behind the shared trait.
