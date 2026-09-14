# ADR 0014: Privacy model for Music Memory and Insights

## Status

Accepted

## Context

VERSIONE may hold intimate records of unreleased work and personal production habits.

## Decision

1. No analytics telemetry to VERSIONE developers or third parties.
2. Insights are local, private, inspectable, and exportable by the user.
3. Remote object storage and Git remotes are explicitly user-configured; nothing auto-uploads statistics.
4. Credentials never enter `.versione/` or Git; absolute personal paths should not be committed as project identity.

## Consequences

- CI and documentation must not imply cloud surveillance features.
- Future optional sync remains opt-in and separate from Insights computation.
