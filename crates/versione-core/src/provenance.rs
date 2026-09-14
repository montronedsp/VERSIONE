//! Provenance for derived or supplied metadata and metrics.
//!
//! VERSIONE records *how* a value was obtained so statistics remain trustworthy.
//! Analyzers and DAW adapters may submit observations later; until then, fields
//! stay unset rather than inventing measurements.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Who or what produced a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MetricSource {
    #[default]
    Manual,
    Versione,
    AudioAnalyzer,
    DawAdapter,
    Import,
}

impl MetricSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Versione => "versione",
            Self::AudioAnalyzer => "audio_analyzer",
            Self::DawAdapter => "daw_adapter",
            Self::Import => "import",
        }
    }
}

/// Optional attribution attached to a stored value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Provenance {
    pub source: MetricSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub analyzer_version: Option<String>,
}

impl Provenance {
    pub fn manual() -> Self {
        Self {
            source: MetricSource::Manual,
            observed_at: Some(Utc::now()),
            source_version: None,
            analyzer_version: None,
        }
    }

    pub fn versione() -> Self {
        Self {
            source: MetricSource::Versione,
            observed_at: Some(Utc::now()),
            source_version: None,
            analyzer_version: None,
        }
    }
}

/// A value paired with how it was obtained.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProvenancedValue<T> {
    pub value: T,
    #[serde(default)]
    pub provenance: Provenance,
}

impl<T> ProvenancedValue<T> {
    pub fn manual(value: T) -> Self {
        Self {
            value,
            provenance: Provenance::manual(),
        }
    }
}
