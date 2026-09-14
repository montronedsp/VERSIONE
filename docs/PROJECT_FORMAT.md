# VERSIONE project format

Git-tracked metadata lives under:

```text
.versione/
```

Large object bytes live in the configured object store (Phase 1 default: `.versione/objects/`). Git commits **never** include object bytes.

## Schema version

```text
format_version = 1
```

Readers must reject unsupported versions cleanly.

## Directory sketch (v1 + Music Memory)

```text
.versione/
  config.toml          # format_version, project_id, hash_algorithm, created_utc
  storage.toml         # local + optional remote settings (no secrets)
  state.toml           # current checkpoint/version tips, next Version number
  profile.toml         # ProjectProfile (format_version independent; lazy)
  notes.toml           # project notes
  tasks.toml           # project tasks
  media.toml           # preview/media metadata (object_id refs only)
  metrics.toml         # provenanced MetricObservation records
  lock                 # exclusive mutation lock (not committed)
  manifests/
    <snapshot-id>.toml
  refs/
    versions.toml      # ordered named Versions (V01, V02, …)
    publication.toml   # remote verified / git pushed hints per snapshot
  metadata/
  objects/             # LocalObjectStore bytes (not committed to Git)
```

Machine-local (not inside the project; rebuildable):

```text
~/.versione/library.toml   # or $VERSIONE_LIBRARY
```

## Content policy (Phase 1)

- Every project file is recorded in the manifest with path, `object_id`, size, and `file_class`.
- **All file bytes** (both `GitEligible` and `LargeObject`) are stored in the object store.
- Preview media bytes use the same object store; only `media.toml` is Git-tracked.
- `FileClass` records classification for future policy; it does not put project files into Git.
- Git commits only VERSIONE metadata (`config`, `storage`, `state`, profile/notes/tasks/media/metrics, `manifests`, `refs`, `metadata`).

## Metadata vs content revisions

- **Content checkpoint** — hashes project tree files into objects + manifest.
- **Metadata revision** — profile/notes/tasks/lifecycle/media-refs/metrics commits without requiring a new content checkpoint.

Both appear in Git history for `.versione/` metadata. They are intentionally separate.

## Compatibility

Older projects without Music Memory files continue to open. Missing profile/notes/tasks/media/metrics use safe empty defaults and may be written lazily. Bump per-document `format_version` on incompatible meaning changes.

## Manifest entries

Sorted by path. Each entry:

```text
path
object_id
size
class
```

Manifest also stores `content_id` (digest of the canonical entry payload) so identical project trees do not create duplicate checkpoints.

## Versions

Display labels (`V01`, `V02`, …) are not permanent identities. Stable identity is `snapshot_id` (UUID).

## Object retention

An object may be deleted only when no retained checkpoint/version requires it and replica policy allows deletion. Phase 1 does **not** implement garbage collection.

## Compatibility

Bump `format_version` on incompatible on-disk meaning changes. Prefer additive fields and migrations.
