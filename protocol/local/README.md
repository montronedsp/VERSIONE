# Local bridge protocol (draft)

Bridges (Max for Live, future helpers) communicate with a local VERSIONE process only.

## Constraints

- Bind to **localhost** only if sockets/HTTP/OSC are used.
- Do not expose a public network service.
- Keep the message surface small: identify project, init, status, keep Version, open UI.
- Validate that callers are local; add auth/token checks if the OS threat model requires it.
- Do not overengineer during foundation — define message names and evolve with an explicit protocol version field.

## Sketch

```text
Client (bridge)  --localhost-->  VERSIONE service
  hello { protocol_version }
  project_identify { path? }
  init_request
  status_request
  keep_version { name }
  open_ui
```

Exact transport (named pipe, UDS, TCP localhost, OSC) is deferred to Phase 6 evaluation on Windows/macOS.
