//! Extensible metrics with provenance.
//!
//! Observations may represent future analyzer outputs (e.g. `audio.clip_events`).
//! VERSIONE does not invent values: absent observations mean “unknown”, not zero.

use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{Error, ErrorKind, Result};
use crate::git::GitRepository;
use crate::project::{read_toml, require_initialized, write_toml_atomic, ProjectPaths};
use crate::provenance::{MetricSource, Provenance};

pub const METRICS_FORMAT_VERSION: u32 = 1;

/// A single typed metric observation with attribution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetricObservation {
    /// Dotted metric name, e.g. `audio.clip_events`, `session.channel_count`.
    pub metric: String,
    pub value: MetricValue,
    #[serde(default)]
    pub provenance: Provenance,
    pub observed_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub related_snapshot_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MetricValue {
    Integer(i64),
    Number(f64),
    Boolean(bool),
    Text(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetricsDocument {
    pub format_version: u32,
    #[serde(default)]
    pub observations: Vec<MetricObservation>,
}

impl MetricsDocument {
    pub fn new_v1() -> Self {
        Self {
            format_version: METRICS_FORMAT_VERSION,
            observations: Vec::new(),
        }
    }
}

/// Well-known metric names reserved for future analyzers / adapters.
pub mod names {
    pub const AUDIO_CLIP_EVENTS: &str = "audio.clip_events";
    pub const AUDIO_PEAK_LEVEL: &str = "audio.peak_level";
    pub const AUDIO_LOUDNESS_LUFS: &str = "audio.loudness_lufs";
    pub const AUDIO_DYNAMIC_RANGE: &str = "audio.dynamic_range";
    pub const AUDIO_CREST_FACTOR: &str = "audio.crest_factor";
    pub const AUDIO_STEREO_WIDTH: &str = "audio.stereo_width";
    pub const SESSION_CHANNEL_COUNT: &str = "session.channel_count";
    pub const SESSION_ARRANGEMENT_DURATION: &str = "session.arrangement_duration_secs";
}

impl ProjectPaths {
    pub fn metrics_path(&self) -> std::path::PathBuf {
        self.versione_dir.join("metrics.toml")
    }
}

pub fn load_or_default_metrics(paths: &ProjectPaths) -> Result<MetricsDocument> {
    if !paths.metrics_path().exists() {
        return Ok(MetricsDocument::new_v1());
    }
    let doc: MetricsDocument = read_toml(&paths.metrics_path())?;
    if doc.format_version != METRICS_FORMAT_VERSION {
        return Err(Error::new(
            ErrorKind::UnsupportedFormat,
            format!("unsupported metrics format_version {}", doc.format_version),
        )
        .with_path(paths.metrics_path()));
    }
    Ok(doc)
}

pub fn save_metrics(paths: &ProjectPaths, doc: &MetricsDocument) -> Result<()> {
    write_toml_atomic(&paths.metrics_path(), doc)
}

/// Record an observation. Does not run analyzers — callers supply measured values.
pub fn record_observation(root: &Path, observation: MetricObservation) -> Result<()> {
    if observation.metric.trim().is_empty() {
        return Err(Error::new(
            ErrorKind::InvalidProject,
            "metric name is required",
        ));
    }
    // Guard against pretending analyzer data without provenance.
    if matches!(
        observation.provenance.source,
        MetricSource::AudioAnalyzer | MetricSource::DawAdapter
    ) && observation.provenance.analyzer_version.is_none()
        && observation.provenance.source_version.is_none()
    {
        return Err(Error::new(
            ErrorKind::InvalidProject,
            "analyzer/adapter observations must record source_version or analyzer_version",
        ));
    }
    let paths = require_initialized(root)?;
    let mut doc = load_or_default_metrics(&paths)?;
    doc.observations.push(observation);
    doc.format_version = METRICS_FORMAT_VERSION;
    save_metrics(&paths, &doc)?;
    let git = GitRepository::new(root);
    let _ = git.commit_versione_metadata("Record metric observation")?;
    Ok(())
}

pub fn list_observations(root: &Path) -> Result<Vec<MetricObservation>> {
    let paths = require_initialized(root)?;
    Ok(load_or_default_metrics(&paths)?.observations)
}

/// Latest observation for a metric name, if any.
pub fn latest_observation(root: &Path, metric: &str) -> Result<Option<MetricObservation>> {
    let mut matches: Vec<_> = list_observations(root)?
        .into_iter()
        .filter(|o| o.metric == metric)
        .collect();
    matches.sort_by_key(|o| o.observed_at);
    Ok(matches.pop())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{init_project, InitOptions};
    use crate::provenance::Provenance;
    use tempfile::tempdir;

    #[test]
    fn records_manual_observation_without_inventing_clips() {
        let dir = tempdir().unwrap();
        init_project(dir.path(), &InitOptions::default()).unwrap();
        assert!(latest_observation(dir.path(), names::AUDIO_CLIP_EVENTS)
            .unwrap()
            .is_none());
        record_observation(
            dir.path(),
            MetricObservation {
                metric: names::AUDIO_CLIP_EVENTS.into(),
                value: MetricValue::Integer(7),
                provenance: Provenance {
                    source: MetricSource::Manual,
                    observed_at: Some(Utc::now()),
                    source_version: None,
                    analyzer_version: None,
                },
                observed_at: Utc::now(),
                unit: Some("events".into()),
                related_snapshot_id: None,
            },
        )
        .unwrap();
        let latest = latest_observation(dir.path(), names::AUDIO_CLIP_EVENTS)
            .unwrap()
            .unwrap();
        assert_eq!(latest.value, MetricValue::Integer(7));
        assert_eq!(latest.provenance.source, MetricSource::Manual);
    }

    #[test]
    fn analyzer_observation_requires_version() {
        let dir = tempdir().unwrap();
        init_project(dir.path(), &InitOptions::default()).unwrap();
        let err = record_observation(
            dir.path(),
            MetricObservation {
                metric: names::AUDIO_CLIP_EVENTS.into(),
                value: MetricValue::Integer(1),
                provenance: Provenance {
                    source: MetricSource::AudioAnalyzer,
                    observed_at: Some(Utc::now()),
                    source_version: None,
                    analyzer_version: None,
                },
                observed_at: Utc::now(),
                unit: None,
                related_snapshot_id: None,
            },
        )
        .unwrap_err();
        assert_eq!(err.kind, ErrorKind::InvalidProject);
    }
}
