# Local object store

Implemented as `versione_core::storage::LocalObjectStore`.

Typical roots (user-configured, not hard-coded in source):

- studio SSD directories
- external drives
- NAS mount points

Objects use content-addressed fan-out paths. Writes stage to a temporary file, verify the digest, then rename into place.
