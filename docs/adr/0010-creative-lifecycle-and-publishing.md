# ADR 0010: Creative lifecycle and publishing pipeline

## Status

Accepted

## Context

Musicians need workflow language beyond backup tips: drafts, ideas worth keeping, active finishing, release work, and archives.

## Decision

1. **Lifecycle** (`draft`, `creative-pool`, `active`, `publishing`, `released`, `archived`) is optional and first-class.
2. **Creative Pool** is intentional: worth preserving, not currently being finished — not a generic “inactive” bucket.
3. **Release stage** is a separate field (`candidate` … `released`) so pipeline progress is not collapsed into lifecycle.
4. Transitions are not rigidly enforced in v1; invalid tokens are rejected at parse time.

## Consequences

- Queries can filter by lifecycle and release stage independently.
- UX copy stays factual (“N projects in Creative Pool…”) and non-judgmental.
