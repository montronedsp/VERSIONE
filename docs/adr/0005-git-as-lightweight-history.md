# ADR 0005: Git as lightweight history layer

- Status: Accepted
- Date: 2026-09-13

## Context

Earlier drafts considered a fully custom history store. Collaboration and GitHub familiarity favor Git for manifests, refs, and version graphs — without abusing Git for huge audio blobs.

## Decision

1. Git is a **core dependency** for lightweight history and remotes (including GitHub).
2. Large binaries use VERSIONE object storage, not normal Git objects.
3. Access Git only through a **narrow internal abstraction**; do not scatter shell calls.
4. Offline local versioning must work when remotes are down.
5. Git LFS is an optional future backend, not the foundation.

Principles resemble git-annex/DVC (Git tracks references; bytes live elsewhere) but VERSIONE owns musician UX and APIs.

## Consequences

- `.versione/` metadata is Git-friendly.
- Publish ordering must not push Git claims before object-store verification (ADR-0006).
