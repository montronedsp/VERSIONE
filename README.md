# VERSIONE

**Backup and version history for music projects.**

VERSIONE is an **experimental**, offline-first backup tool for DAW sessions, recordings, samples, and related project files. It is intended for development and evaluation only — **not for production use** and **not a substitute for a proven backup routine**.

License: [GNU General Public License v3.0](LICENSE) (`GPL-3.0-only`).

---

## Experimental notice

This software is early and unstable. Formats, commands, and behavior may change without a migration path. Do not rely on it as your only copy of important work. Always keep independent backups.

---

## What it is

VERSIONE works like a specialized backup / version manager for creative projects:

| You get | Meaning |
|---------|---------|
| **Local copies you control** | Data stays on your machine by default |
| **Checkpoints** | Point-in-time snapshots of a project tree |
| **Named versions** | Milestones you can restore later |
| **Safe restore** | Open a previous state as a **copy** — no silent overwrite |
| **Deduplicated storage** | Unchanged files are stored once |
| **Tagging and search** | Find projects by tags, names, and other metadata you entered |
| **Optional remote copy** | You choose where (if anywhere) a second copy goes |

VERSIONE does **not** sell cloud storage, does **not** require an account, and does **not** run a VERSIONE online service for your files.

---

## No AI

VERSIONE is a **good old backup application** with tagging and search. There is **no AI** of any kind:

- no AI workflow inference
- no recommendations engines
- no language models
- no automatic “what should I finish next” scoring
- no cloud AI features

Organization is manual and deterministic: you tag projects, set status, search and filter what you stored. That is all.

---

## Offline and self-managed

- Works **fully offline** for local backup, history, and restore.
- You own the disks, folders, and credentials.
- No mandatory cloud, subscription, or phone-home analytics.

If you want an offsite copy, **you** connect storage or Git remotes of your choice. VERSIONE only provides the **organization layer** (how projects are snapshotted, indexed, and restored). The hosting is yours.

Examples of optional destinations you may configure yourself:

- A second drive or NAS path
- An S3-compatible bucket (AWS S3, Cloudflare R2, MinIO, Backblaze B2 S3 API, …)
- A Git remote such as GitHub/GitLab for **metadata history** (not for dumping multi-gigabyte audio into normal Git)

VERSIONE never uploads to “VERSIONE Cloud.” There is none.

---

## Optional visual project memory

Screenshots can be attached to a snapshot as **optional visual context**. They enrich history in the desktop app later, but they are **not** part of reconstructability.

```bash
versione screenshot add path/to/view.png path/to/project --label "Mix view"
versione screenshot list path/to/project
```

- Bytes live in the object store (BLAKE3, deduplicated).
- Metadata is recorded in `.versione/media.toml` (Git).
- Missing or corrupt screenshots do **not** invalidate a musical snapshot.
- Capture is **explicit import only** in this release (no DAW/desktop automation).

---

## How backup is organized

```text
Your music project folder
        │
        ▼
   VERSIONE checkpoint / version
        │
        ├── Object store  →  project file bytes (local; optional remote you configure)
        └── Metadata      →  manifests & history (Git under the hood, for bookkeeping)
```

- **Object store** holds the actual project files (content-addressed).
- **Metadata** records what belonged to each checkpoint or named version.
- Large media are **not** stuffed into ordinary Git blobs as the primary storage model.

---

## Typical use

```bash
# Build (Rust toolchain + git required)
cargo build -p versione-cli --release

# Prefer enable when a project becomes worth preserving:
versione enable path/to/My\ Track
# or: versione enable path/to/My\ Track/Track.als
# or: versione enable path/to/Song.rpp
# optional (Ableton / extras): --collect /path/to/external.wav

versione status path/to/project
versione verify path/to/project
versione snapshot path/to/project
versione version "Mix revision" path/to/project
versione history path/to/project
versione restore V01 --to path/to/Restored-V01 path/to/project

# Lower-level init/checkpoint remain available for development:
versione init path/to/project
versione checkpoint path/to/project
```

> **When a project matters, enable VERSIONE.**

---

## Supported DAWs (current)

| DAW | Project detection | Media-reference discovery | Self-contained snapshots |
|-----|-------------------|---------------------------|---------------------------|
| **REAPER** | `.rpp` file or unambiguous folder | Automatic via read-only RPP `FILE` inspection | Yes — externals collected; staged RPP rewrite ingested (working `.rpp` untouched) |
| **Ableton** | `.als` file or unambiguous folder | Not yet (use `--collect` or keep assets project-local) | Partial — lifecycle works; Set parsing is roadmap |
| **Generic** | Any folder | Manual `--collect` only | Snapshot of scanned tree after verification |

### REAPER details

Supported reference discovery targets normal `FILE` directives (typically under `SOURCE WAVE` / audio-like sources, and other explicit FILE lines). Paths resolve relative to the `.rpp` directory (not the process CWD).

**Supported for self-containment today:** file-backed media referenced by `FILE` that can be copied into `Media/imported/` and rewritten in the **staged** snapshot `.rpp`.

**Not claimed yet:** full RPP grammar, FX/plugin assets, preference mutation, launching REAPER, or treating every quoted string as media.

### Ableton details

Ableton project/root support and the VERSIONE lifecycle work. External-reference discovery inside `.als` is still limited — supply `--collect` or keep samples inside the project folder.

---

## Project lifecycle

VERSIONE is designed to stay out of the way while you make music.

You continue working normally inside your DAW. When a project becomes important enough to preserve, you **enable VERSIONE** for that project.

From that point forward, VERSIONE is responsible for helping make the project **self-contained, verified, and versioned**.

The core model is:

```text
self-contained project
        ↓
verified integrity
        ↓
reproducible snapshot
        ↓
version history
```

### Ideal workflow

1. You are working normally inside a project such as `My Track/`.
2. Hit **Enable VERSIONE** (`versione enable`).
3. VERSIONE identifies the project root (Ableton `.als`, REAPER `.rpp`, or generic folder).
4. Collection brings supported external assets into a controlled project-local `Media/` area (automatic for discovered REAPER `FILE` refs; explicit `--collect` elsewhere).
5. VERSIONE scans the project and builds a manifest containing project files, audio/media, supported project-local assets, sizes, BLAKE3 hashes, and the VERSIONE project ID.
6. VERSIONE verifies that every expected object exists and that its content matches the manifest (for REAPER, staged self-contained `.rpp` bytes are verified).
7. Only after successful verification is the first VERSIONE snapshot created.
8. Future snapshots ingest only **new or changed objects** instead of blindly duplicating the entire project.

Lifecycle states:

```text
UNMANAGED → COLLECTED → VERIFIED → VERSIONED → PUBLISHED
```

`PUBLISHED` means that the snapshot metadata and every object required by its manifest have also been verified on the configured remote storage, and Git metadata push succeeded. VERSIONE never silently uploads project media.

### Collection is not snapshotting

The initial collection step and normal VERSIONE snapshots are intentionally different operations.

Making a project self-contained may require collecting external audio into the project. That should normally happen when VERSIONE is first enabled, or when the user explicitly requests another collection pass (`--collect`).

VERSIONE does **not** blindly perform a full collection operation for every snapshot.

After initialization, content-addressed storage allows unchanged media to be reused. New snapshots only need to ingest objects whose content has changed or which did not previously exist.

### Verification before history

A VERSIONE snapshot is intended to represent a project that can actually be reconstructed later.

Before a snapshot becomes valid, VERSIONE verifies the objects described by its working tree / manifest:

```text
expected object
      ↓
object exists
      ↓
size matches
      ↓
BLAKE3 hash matches
      ↓
verified
```

If required media is missing or corrupted, the project must not be presented as successfully verified. A failed integrity check blocks the snapshot.

### Git and media storage

VERSIONE separates lightweight project history from large media storage.

Git can be used for suitable metadata, manifests, references, and collaborative history.

Large audio and other media objects belong to VERSIONE's content-addressed object-storage layer rather than being treated as ordinary Git blobs.

Conceptually:

```text
Music Project
│
├── Track.als
├── Media/
│   └── imported/          # assets collected by VERSIONE (when requested)
│
└── .versione/
    ├── config.toml
    ├── lifecycle.toml
    ├── collection.toml
    ├── manifests/
    ├── refs/
    ├── state/
    └── objects/           # local content-addressed store (never Git blobs)
```

The storage location remains the user's decision. Projects may remain completely local or use a supported NAS, S3-compatible service, or other configured remote.

VERSIONE must never require a proprietary VERSIONE cloud in order to preserve a project.

### Current collection limitations

**Ableton:** Live Set (`.als`) reference extraction is **not** implemented yet. Automatic “Collect All and Save” discovery inside a Set remains **roadmap behavior**. Use `--collect` or keep assets project-local.

**REAPER:** `FILE` media references are discovered automatically. The working `.rpp` is never rewritten; VERSIONE stages a self-contained copy for the snapshot. Missing/unsafe references block verification rather than claiming success.

Today, `versione enable`:

- resolves Ableton / REAPER / generic project roots;
- initializes `.versione/` idempotently;
- for REAPER: inspects `.rpp`, collects external `FILE` media, stages a rewritten snapshot `.rpp`;
- for Ableton/generic: copies explicit `--collect` paths into `Media/imported/` (copy + BLAKE3 verify; never moves/deletes sources);
- scans and verifies the project tree (using staged bytes where overrides exist);
- creates the first snapshot only after verification succeeds.

Assets already inside the project folder are left in place and included in the snapshot scan.

---

## Optional remote copy

Optional remote object storage (credentials via environment variables only — never written into the project):

```bash
versione storage set-remote path/to/project \
  --bucket my-bucket \
  --region auto \
  --endpoint https://your-provider.example

versione publish path/to/project
```

Publish verifies remote objects first, then pushes metadata. Incomplete remote copies are not treated as “backed up.”

---

## Desktop (experimental)

An early local browser for known projects:

```bash
cargo run -p versione-app --release
```

Window title: **VERSIONE**. Same offline rules: library and project data stay on your computer unless you configure remotes yourself.

---

## What VERSIONE is not

- Not a DAW
- Not a cloud subscription
- Not automatic “set and forget” disaster recovery for every disk failure
- Not affiliated with Git, GitHub, or any storage vendor

---

## Build checks

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo build --workspace --all-features
```

Linux desktop builds may need system packages such as `libasound2-dev` and `libgtk-3-dev`.

---

## Documentation

- [Architecture](docs/ARCHITECTURE.md)
- [Storage](docs/STORAGE.md)
- [Safety](docs/SAFETY.md)
- [ADRs](docs/adr/)

---

## License

[GNU General Public License v3.0](LICENSE)
