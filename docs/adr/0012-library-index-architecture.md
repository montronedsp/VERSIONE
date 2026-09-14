# ADR 0012: Library index is rebuildable, not authoritative

## Status

Accepted

## Context

Musicians need cross-project search (“what did I forget?”) without a proprietary catalog becoming the source of truth.

## Decision

1. Each project remains self-describing under `.versione/`.
2. The library (`~/.versione/library.toml` or `VERSIONE_LIBRARY`) stores registered roots and optional smart collections (saved queries).
3. Search/statistics read project metadata via summaries; they do not hash audio to answer metadata queries.
4. If the index is deleted, projects still open; `library rebuild` / re-register restores the index.

## Consequences

- Portability and integrity do not depend on a central database.
- Corruption recovery = delete index and rebuild from known roots.
