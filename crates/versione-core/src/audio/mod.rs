//! DAW-independent audio analysis.

pub mod analyzer;

pub use analyzer::{
    analyze_wav, record_observation, AudioAnalysis, AudioAnalyzerProvenance, ClipEvent,
    ANALYZER_VERSION,
};
