# ADR 0009: Stable project identity and portable profile

## Status

Accepted

## Context

Projects are renamed and directories move. Path-based identity breaks library indexing, statistics, and restore relationships.

## Decision

1. `ProjectId` (UUID in `config.toml`) remains the durable identity.
2. Creative metadata lives in `.versione/profile.toml` with its own `format_version`, carrying the same `project_id`.
3. Profile data travels with the project. The machine-local library index is rebuildable and never authoritative.
4. Metadata edits create Git metadata revisions independently of content checkpoints.

## Consequences

- Renaming/moving a folder does not change `project_id`.
- Older projects without `profile.toml` lazily default until first write.
- GUI and CLI share `ProjectSummary` / `ProjectProfile` domain types.
