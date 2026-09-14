# ADR 0003: Checkpoint vs Version vs Published

- Status: Accepted
- Date: 2026-09-13

## Context

Musicians need frequent recoverable states, named milestones, and a clear notion of “safely shared / backed up” without equating those ideas.

## Decision

| Concept | Meaning |
|---------|---------|
| Checkpoint | Automatic/lightweight recoverable state |
| Version | Explicit named state kept by the musician |
| Published Version | Version whose required large objects are verified in the configured remote object store **and** whose Git history was pushed successfully |

Promotion Checkpoint → Version must not duplicate object bytes. Publication is a separate transaction (ADR-0006).

## Consequences

- UI must show layer availability (local / object store / Git remote) honestly.
- `published == true` only when all required layers succeed.
