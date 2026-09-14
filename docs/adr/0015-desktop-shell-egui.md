# ADR 0015: Desktop shell uses egui/eframe

## Status

Accepted

## Context

VERSIONE needs a cross-platform musician-facing Studio on Windows, macOS, and Linux. Domain logic must remain in Rust (`versione-core`). Electron was rejected: high memory cost, duplicate web stack, and weaker fit for a calm local archive tool.

## Decision

1. Use **egui + eframe** for the first VERSIONE Studio shell (`crates/versione-app`).
2. The window title and product name remain **VERSIONE**; the crate may be `versione-app`.
3. All library/search/stats/workflow mutations call `versione-core` / `services` APIs — the UI never writes TOML directly.
4. Audio playback uses a Rust crate (`rodio`) operating on object-backed preview files exported to a temp/cache path when needed.

## Consequences

- One language for domain + UI reduces logic drift.
- Packaging is simpler than Tauri for an early Studio (no separate web toolchain in CI).
- Visual design must be intentionally typographic and restrained; egui defaults are customized.
- A future Tauri shell remains possible if web designers join; the service layer stays reusable.
