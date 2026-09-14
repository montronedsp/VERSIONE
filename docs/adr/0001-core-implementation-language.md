# ADR 0001: Core implementation language

- Status: Accepted
- Date: 2026-09-13

## Context

VERSIONE needs filesystem-safe local tooling, Git integration, streaming hashes for multi-gigabyte audio, pluggable object stores (local + S3-compatible), concurrency between watcher/UI/publish, and high-stakes restore/publish safety.

## Decision

Implement **VERSIONE Core and CLI in Rust** (edition 2021+, stable toolchain).

UI toolkits are evaluated later for `apps/versione-desktop`. The storage and Git layers must not depend on a GUI framework.

## Consequences

- Cargo workspace: `versione-core`, `versione-cli`.
- CI: fmt, clippy, test on major desktop OSes.
- Dependencies audited for GPLv3 compatibility before adoption.
