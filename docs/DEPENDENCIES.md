# Third-party dependencies

VERSIONE is GPLv3. Dependencies must remain license-compatible.

| Crate | Role | Notes |
|-------|------|-------|
| `blake3` | Content hashing | Permissive |
| `clap` | CLI parsing | Permissive |
| `serde` / `toml` | Config and manifests | Permissive |
| `thiserror` | Error types | Permissive |
| `uuid` | Project/snapshot ids | Permissive |
| `chrono` | Timestamps | Permissive |
| `ureq` | Sync HTTP for S3-compatible APIs | Permissive |
| `hmac` / `sha2` / `hex` | AWS SigV4 signing | Permissive |
| `urlencoding` | S3 key path encoding | Permissive |
| `tempfile` | Tests only | Permissive |

Git integration invokes the system `git` executable through a centralized module.

Credentials for remote storage are never stored in crate configuration files.
