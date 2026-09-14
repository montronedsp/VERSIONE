# ADR 0002: Content-addressed object identity

- Status: Accepted
- Date: 2026-09-13

## Context

Large media must deduplicate across Versions and verify integrity on restore without exotic cryptography or early delta compression.

## Decision

1. Identify immutable bytes by **BLAKE3** digest (`hash_algorithm = "blake3"`).
2. Hash with **streaming I/O**.
3. Keep `ObjectId` independent of storage location / provider URL.
4. Defer chunked/delta compression.

## Consequences

- Local stores may use fan-out paths `ab/cdef…` keyed by digest.
- Restore and publish verify digests before claiming success.
