# Common DAW adapter notes

Adapters translate DAW-specific project layout into core concepts:

- project root discovery
- ignore defaults
- optional metadata
- optional semantic diff

Adapters must not implement object storage or history publication. Those remain in `versione-core`.
