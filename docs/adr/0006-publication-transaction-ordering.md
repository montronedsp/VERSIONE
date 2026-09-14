# ADR 0006: Publication transaction ordering

- Status: Accepted
- Date: 2026-09-13

## Context

Falsely advertising a remotely recoverable Version is worse than refusing to publish.

## Decision

Canonical publish sequence:

1. Scan project  
2. Hash contents  
3. Write missing objects to local store  
4. Verify local objects  
5. Upload required objects to remote object store  
6. Verify remote availability  
7. Write/update VERSIONE manifest  
8. Create Git commit  
9. Push Git remote  
10. Mark Version published  

Gates:

- Object-store verification **before** Git remote publish claims.
- Failed upload → local Version valid; not published.
- Failed Git push → local Version valid; not published.

## Consequences

`gate_git_push` (and successors) encode this invariant in code and tests.
