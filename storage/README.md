# Storage backends

Backend-specific notes and future crates live here. Runtime interfaces are defined in `crates/versione-core` (`storage` module).

| Directory | Intent |
|-----------|--------|
| `local/` | Local/NAS/external paths — **implemented in core** |
| `s3/` | S3-compatible remote — planned |
| `webdav/` | Extension point |
| `rclone/` | Extension point |
| `git-lfs/` | Optional backend only |
| `common/` | Shared backend documentation |
