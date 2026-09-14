# VERSIONE Desktop

The musician-facing Studio lives in `crates/versione-app` (binary `versione-studio`).

```bash
cargo run -p versione-app --release
```

It consumes `versione-core` application services and never reimplements CAS, manifests, or restore validation. GUI technology decision: [ADR 0015](../../docs/adr/0015-desktop-shell-egui.md).
