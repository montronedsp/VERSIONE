# Ableton adapter (planned)

This directory will hold Ableton Live–specific discovery, ignore defaults, and optional read-only metadata extraction.

Rules:

- Do not mutate `.als` Sets.
- Treat `.als` as opaque authoritative bytes for snapshot/restore.
- Semantic parsing is optional and must not be required for versioning success.
- Keep Live version differences defensive; fail soft on unsupported structures.

Implementation belongs in later Phase 4 work, ideally as a Rust crate or module that depends on `versione-core` APIs without leaking Ableton types upward.
