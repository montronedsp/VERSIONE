# Roadmap

## Phase 0 — Foundation

License, docs, ADRs, Rust workspace, LocalObjectStore, Git abstraction, publish gates, CI.

**Status:** complete.

## Phase 1 — Core CLI workflow

`init`, `status`, `checkpoint`, `version`, `history`, `restore --to` with local object store and Git metadata commits.

**Status:** complete for local offline use.

## Phase 2 — Remote storage and publication

S3-compatible backend, replica/publication tracking, publish gate (objects before Git remote), `versione publish`, failure/retry tests.

**Status:** complete for S3-compatible remotes + explicit publish.

## Phase 3 — Watcher

Debounced filesystem monitoring and automatic checkpoints.

## Phase 4 — Ableton adapter

Opaque `.als` preservation, discovery, adapter ignore defaults.

## Phase 5 — Desktop

Musician-facing UI for history, storage availability, publish, restore-as-copy.

## Phase 6 — Max bridge

Thin localhost bridge device.

## Phase 7 — Semantic diff

Adapter-local music diffs.

## Phase 8 — Additional DAWs

Validate adapter boundary beyond Ableton.
