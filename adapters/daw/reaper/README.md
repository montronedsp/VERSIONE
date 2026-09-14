# REAPER adapter notes

VERSIONE implements a **read-only** `.rpp` inspector in `versione-core` (`daw::reaper`).

## Supported today

- Project identification via `.rpp` (file or unambiguous directory)
- Discovery of `FILE` directives inside `SOURCE` chunks (and other FILE lines)
- Classification: project-local / external / missing / unsafe
- Collection of external media into `Media/imported/`
- Staged self-contained `.rpp` rewrite under `.versione/staging/` (canonical working `.rpp` untouched)
- Snapshot ingestion of staged bytes via content overrides

## Not supported yet

- Launching REAPER or depending on a REAPER install
- Mutating the user's canonical `.rpp`
- Full RPP AST fidelity / round-trip pretty-print
- Plugin state, FX presets as media, or opaque binary blobs
- Automatic collection of every conceivable path-like string (only `FILE` directives)
- Nested subproject (`.rpp` referenced as media) deep collection graphs beyond FILE discovery

Keep adapter-specific logic here conceptually; core storage/manifest/publish remain DAW-agnostic.
