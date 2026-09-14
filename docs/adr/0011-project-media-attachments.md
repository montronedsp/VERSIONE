# ADR 0011: Project media attachments

## Status

Accepted

## Context

Audio/arrangement previews help rediscovery. Large binaries must not enter Git.

## Decision

1. Preview bytes use the existing content-addressed object store.
2. `.versione/media.toml` stores `ProjectMedia` metadata (`kind`, `object_id`, mime, labels).
3. Kinds are closed but extensible: `audio_preview`, `arrangement_image`, `reference`, `artwork`, `document`, `other`.
4. No DAW screen scraping or automatic Ableton extraction in this milestone.

## Consequences

- Deduplication works like other project files.
- Missing objects are reported safely without crashing search.
- Git history for media is metadata-only.
