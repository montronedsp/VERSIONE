# Safety invariants

1. Never modify source project files during snapshot creation.
2. Never silently overwrite working DAW files.
3. Never delete history automatically.
4. Never trust stored paths without validation.
5. Never allow restore path traversal / destination escape.
6. Verify object hashes before restore.
7. A failed snapshot cannot corrupt previous history.
8. A failed object upload cannot create a falsely published Version.
9. A failed Git push must leave the local Version valid.
10. GitHub unavailable must never prevent local versioning.
11. Object storage unavailable must never destroy local state.
12. Never delete the last known valid copy of an object automatically.

## Publication order

Scan → hash → local objects → verify local → upload remote objects → verify remote → write manifest → Git commit → push → mark published.

If upload fails: local Version stays valid; status must show object store / GitHub incomplete.

## Restore

Default: **Open as Copy**. Destructive in-place restore waits on proven transactions.

## Logging

No audio contents, credentials, or unnecessary absolute personal paths in logs.
