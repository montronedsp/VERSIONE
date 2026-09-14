# ADR 0013: Insights metrics and provenance

## Status

Accepted

## Context

Future production statistics (clipping, loudness, DAW-derived counts) must not pollute Insights with invented numbers.

## Decision

1. `MetricObservation` stores named metrics with `MetricSource` provenance (`manual`, `versione`, `audio_analyzer`, `daw_adapter`, `import`).
2. Analyzer/adapter observations require a source/analyzer version string.
3. Aggregate Insights only use data VERSIONE actually has (profiles, tasks, versions, recorded observations).
4. Reserved metric names (e.g. `audio.clip_events`) exist in schema now; automatic detection stays inactive until a real analyzer exists.

## Consequences

- “Times clipped” can be modeled without fake detectors.
- Missing data stays missing (not coerced to zero unless the statistic is a count of known rows).
