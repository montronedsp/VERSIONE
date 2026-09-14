# Object storage

Large music assets use VERSIONE’s pluggable object store. Git tracks lightweight references only.

## Interface

```text
ObjectStore
  has(ObjectId)
  put(ObjectId, stream)
  get(ObjectId)
  verify(ObjectId)
  remove(ObjectId)   # conservative; not used for GC yet
  metadata(ObjectId)
```

`ObjectId` is a BLAKE3 digest. Provider URLs are never part of the id.

## Backends

| Backend | Status |
|---------|--------|
| Local (`LocalObjectStore`) | Implemented |
| S3-compatible (`S3ObjectStore`) | Implemented |
| In-memory (`MemoryObjectStore`) | Test double |
| WebDAV / rclone / Git LFS | Extension points only |

### S3-compatible configuration

Non-secret settings live in `.versione/storage.toml` (`remote` table): bucket, region, optional endpoint, prefix, location id.

Credentials are loaded from the environment only:

- `VERSIONE_S3_ACCESS_KEY_ID` or `AWS_ACCESS_KEY_ID`
- `VERSIONE_S3_SECRET_ACCESS_KEY` or `AWS_SECRET_ACCESS_KEY`
- optional session token via `VERSIONE_S3_SESSION_TOKEN` / `AWS_SESSION_TOKEN`

Never commit credentials.

### Remote integrity model

After upload, VERSIONE confirms the object exists (HEAD) and that `Content-Length` matches the expected size from the local object/manifest. S3 ETag is **not** treated as MD5 or as VERSIONE’s content digest. Full remote re-hash on every publish is deferred; local restore continues to re-hash every byte.

## Publication

`versione publish` uploads missing remote objects, verifies all snapshot objects remotely, records publication state, then pushes Git metadata. If remote verification is incomplete, Git is not pushed.

## Object retention

An object may be deleted only when no retained checkpoint/version requires it and replica policy allows deletion. Phase 2 does not implement garbage collection or user-facing remote delete.
