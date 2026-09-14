# ADR 0008: Remote publish requires verified objects

- Status: Accepted
- Date: 2026-09-13

## Context

VERSIONE uses Git for metadata and a separate object store for project bytes. Pushing Git history that references missing remote objects would advertise a non-recoverable “published” state.

## Decision

1. `versione publish` uploads missing objects to the configured remote `ObjectStore`.
2. Every manifest object must pass remote existence + size verification.
3. Only then may VERSIONE push Git metadata (`git push`, never `--force`).
4. Cached publication records are hints; remote verification always runs again.
5. Objects may remain remotely after a failed Git push; retries reuse them.

## Consequences

- Publication can be incomplete (remote OK, Git not pushed) without corrupting local history.
- Status must not say “published” unless remote verification and Git push both succeeded.
