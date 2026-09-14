# Architecture

VERSIONE coordinates three layers:

```text
Git / GitHub     lightweight history, manifests, metadata, collaboration
Object storage   large music binaries (local, NAS, S3-compatible, …)
Core + adapters  musician model, safety, DAW discovery
```

This is **not** “Git LFS with a GUI.” VERSIONE owns the large-object abstraction. Git LFS may later be one optional backend.

## Layout

```text
apps/versione-desktop/     # musician UI (planned)
crates/versione-core/      # library modules below
crates/versione-cli/       # development CLI
storage/                   # backend docs / future crates
  local/ s3/ webdav/ git-lfs/ rclone/
git/                       # Git integration notes
adapters/daw/              # DAW adapters (Ableton first)
bridge/ableton-max/        # thin Max bridge (planned)
protocol/local/            # localhost bridge protocol
docs/adr/
```

### `versione-core` modules

| Module | Role |
|--------|------|
| `project` | `.versione/` init and config (no credentials) |
| `manifest` | snapshot manifests (Git-friendly) |
| `classify` | small vs large file policy |
| `objects` | BLAKE3 identity / streaming hash |
| `storage` | `ObjectStore` trait, locations, `LocalObjectStore` |
| `git` | narrow Git repository abstraction |
| `publish` | availability + publication ordering gates |
| `history` | Checkpoint / Version / published flags |
| `restore` | open-as-copy planning |
| `diff` | generic manifest diff |
| `filesystem` / `ignore` | path safety and exclusions |

## Data flow

```text
DAW project files
  → classify (git-eligible vs large-object)
  → stream-hash → ObjectId
  → LocalObjectStore / remote ObjectStore (large)
  → manifest + metadata (Git)
  → optional publish: verify objects → Git commit → push → mark published
```

## Related docs

- [GIT_MODEL.md](GIT_MODEL.md)
- [STORAGE.md](STORAGE.md)
- [PROJECT_FORMAT.md](PROJECT_FORMAT.md)
- [SAFETY.md](SAFETY.md)
- [ROADMAP.md](ROADMAP.md)
