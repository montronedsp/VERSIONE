# ADR 0004: Generic DAW adapter boundary

- Status: Accepted
- Date: 2026-09-13

## Context

Ableton is first, but core storage and Git history must not encode Ableton XML or Max internals.

## Decision

Core versions opaque trees + manifests. Adapters provide discovery, ignore hints, optional read-only metadata, optional semantic diff. Snapshot success must not require semantic parse. Bridges are optional localhost clients.

## Consequences

No Ableton types in `versione-core` public API. Phase order remains storage/Git-core first, Ableton second.
