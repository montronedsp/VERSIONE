# VERSIONE Bridge (Max for Live) — planned

Optional thin Max for Live device. No DSP. No audio processing.

Eventual responsibilities:

- identify active Live project/set
- detect VERSIONE initialization
- request init / keep Version / open VERSIONE
- show simple status (NOT INITIALIZED / WATCHING)
- talk only to a local VERSIONE process via `protocol/local`

Do not build a complex device in early phases. Prefer a stub and protocol agreement first.
