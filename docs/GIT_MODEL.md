# Git model

Git is a **first-class dependency** for VERSIONE history and collaboration.

## What Git stores

- `.versione/` config and metadata (never credentials)
- manifests and object *references*
- version / checkpoint records
- branch / variation graph
- small Git-eligible project files when policy allows

## What Git must not store

- Multi-gigabyte WAV/AIFF/FLAC recordings and similar media as normal Git blobs
- Cloud access keys or GitHub tokens

## Musician UX

Do not expose staging, detached HEAD, plumbing, or raw SHAs in musician UI. Internally VERSIONE uses a narrow abstraction (`GitRepository`, `GitCommit`, `GitRemote`, `GitStatus`) with centralized process/backend calls.

## Offline

Local Checkpoints and Versions must work when GitHub is unreachable. A failed remote push leaves local history valid.

Phase 1 commits VERSIONE metadata locally after successful checkpoint/Version publication. No remote push is performed.

## GitHub

A project may connect to a Git hosting remote. GitHub (or any Git remote) holds the version graph and lightweight files. Large recoverability requires the configured object store.

Phase 2 `versione publish` pushes Git metadata only after remote object verification succeeds.

## Git LFS

Optional future `GitLfsObjectStore` backend only. Not required for architecture or v1 design.
