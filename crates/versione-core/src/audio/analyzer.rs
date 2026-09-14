//! PCM WAV analyzer.
//!
//! Clip events are contiguous runs of full-scale samples in the interleaved PCM
//! stream. A run of adjacent full-scale samples is reported as one event.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::error::{Error, ErrorKind, Result};
use crate::metrics::{names, record_observation as record_metric, MetricObservation, MetricValue};
use crate::provenance::{MetricSource, Provenance};

pub const ANALYZER_VERSION: &str = "versione-audio-1.0";

/// Provenance marker for audio analyzer output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioAnalyzerProvenance {
    pub analyzer_version: String,
}

impl Default for AudioAnalyzerProvenance {
    fn default() -> Self {
        Self {
            analyzer_version: ANALYZER_VERSION.into(),
        }
    }
}

/// One contiguous full-scale run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClipEvent {
    pub start_sample: u64,
    pub sample_count: u64,
}

/// DAW-independent PCM analysis. Loudness/LUFS is intentionally absent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioAnalysis {
    pub duration_seconds: f64,
    pub sample_rate: u32,
    pub channels: u16,
    pub sample_peak: f64,
    pub clip_events: Vec<ClipEvent>,
    pub rms: f64,
    pub crest_factor: Option<f64>,
    pub total_samples: u64,
    pub provenance: AudioAnalyzerProvenance,
}

pub fn analyze_wav(path: &Path) -> Result<AudioAnalysis> {
    let mut file = File::open(path).map_err(|e| Error::io("failed to open WAV file", path, e))?;
    let mut riff = [0u8; 12];
    file.read_exact(&mut riff)
        .map_err(|e| Error::io("failed to read WAV header", path, e))?;
    if &riff[0..4] != b"RIFF" || &riff[8..12] != b"WAVE" {
        return Err(
            Error::new(ErrorKind::UnsupportedFormat, "expected RIFF/WAVE file").with_path(path),
        );
    }

    let mut fmt = None;
    let mut data = Vec::new();
    loop {
        let mut header = [0u8; 8];
        match file.read_exact(&mut header) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(Error::io("failed to read WAV chunk header", path, e)),
        }
        let chunk_id = &header[0..4];
        let chunk_size = u32::from_le_bytes(header[4..8].try_into().expect("fixed size")) as u64;
        if chunk_id == b"fmt " {
            let mut bytes = vec![0u8; chunk_size as usize];
            file.read_exact(&mut bytes)
                .map_err(|e| Error::io("failed to read WAV fmt chunk", path, e))?;
            fmt = Some(parse_fmt(path, &bytes)?);
        } else if chunk_id == b"data" {
            data.resize(chunk_size as usize, 0);
            file.read_exact(&mut data)
                .map_err(|e| Error::io("failed to read WAV data chunk", path, e))?;
        } else {
            file.seek(SeekFrom::Current(chunk_size as i64))
                .map_err(|e| Error::io("failed to skip WAV chunk", path, e))?;
        }
        if chunk_size % 2 == 1 {
            file.seek(SeekFrom::Current(1))
                .map_err(|e| Error::io("failed to skip WAV padding", path, e))?;
        }
    }

    let fmt = fmt.ok_or_else(|| {
        Error::new(ErrorKind::UnsupportedFormat, "WAV fmt chunk not found").with_path(path)
    })?;
    if data.is_empty() {
        return Err(
            Error::new(ErrorKind::UnsupportedFormat, "WAV data chunk not found").with_path(path),
        );
    }
    analyze_pcm(path, &fmt, &data)
}

/// Record analyzer observations with `AudioAnalyzer` provenance.
pub fn record_observation(root: &Path, analysis: &AudioAnalysis) -> Result<()> {
    let now = Utc::now();
    let provenance = Provenance {
        source: MetricSource::AudioAnalyzer,
        observed_at: Some(now),
        source_version: None,
        analyzer_version: Some(ANALYZER_VERSION.into()),
    };
    let observations = [
        (
            "audio.duration_seconds",
            MetricValue::Number(analysis.duration_seconds),
            Some("seconds"),
        ),
        (
            "audio.sample_rate",
            MetricValue::Integer(i64::from(analysis.sample_rate)),
            Some("hz"),
        ),
        (
            "audio.channels",
            MetricValue::Integer(i64::from(analysis.channels)),
            Some("channels"),
        ),
        (
            names::AUDIO_PEAK_LEVEL,
            MetricValue::Number(analysis.sample_peak),
            Some("ratio"),
        ),
        (
            names::AUDIO_CLIP_EVENTS,
            MetricValue::Integer(analysis.clip_events.len() as i64),
            Some("events"),
        ),
        (
            "audio.rms",
            MetricValue::Number(analysis.rms),
            Some("ratio"),
        ),
    ];

    for (metric, value, unit) in observations {
        record_metric(
            root,
            MetricObservation {
                metric: metric.into(),
                value,
                provenance: provenance.clone(),
                observed_at: now,
                unit: unit.map(str::to_string),
                related_snapshot_id: None,
            },
        )?;
    }
    if let Some(crest) = analysis.crest_factor {
        record_metric(
            root,
            MetricObservation {
                metric: names::AUDIO_CREST_FACTOR.into(),
                value: MetricValue::Number(crest),
                provenance,
                observed_at: now,
                unit: Some("ratio".into()),
                related_snapshot_id: None,
            },
        )?;
    }
    Ok(())
}

#[derive(Debug, Clone, Copy)]
struct WavFmt {
    audio_format: u16,
    channels: u16,
    sample_rate: u32,
    block_align: u16,
    bits_per_sample: u16,
}

fn parse_fmt(path: &Path, bytes: &[u8]) -> Result<WavFmt> {
    if bytes.len() < 16 {
        return Err(
            Error::new(ErrorKind::UnsupportedFormat, "WAV fmt chunk too short").with_path(path),
        );
    }
    let fmt = WavFmt {
        audio_format: u16::from_le_bytes(bytes[0..2].try_into().expect("fixed size")),
        channels: u16::from_le_bytes(bytes[2..4].try_into().expect("fixed size")),
        sample_rate: u32::from_le_bytes(bytes[4..8].try_into().expect("fixed size")),
        block_align: u16::from_le_bytes(bytes[12..14].try_into().expect("fixed size")),
        bits_per_sample: u16::from_le_bytes(bytes[14..16].try_into().expect("fixed size")),
    };
    if fmt.audio_format != 1 {
        return Err(Error::new(
            ErrorKind::UnsupportedFormat,
            "only PCM WAV format is supported",
        )
        .with_path(path));
    }
    if fmt.channels == 0 || fmt.sample_rate == 0 || fmt.block_align == 0 {
        return Err(
            Error::new(ErrorKind::UnsupportedFormat, "invalid WAV fmt values").with_path(path),
        );
    }
    match fmt.bits_per_sample {
        8 | 16 | 24 | 32 => Ok(fmt),
        bits => Err(Error::new(
            ErrorKind::UnsupportedFormat,
            format!("unsupported PCM bit depth: {bits}"),
        )
        .with_path(path)),
    }
}

fn analyze_pcm(path: &Path, fmt: &WavFmt, data: &[u8]) -> Result<AudioAnalysis> {
    let bytes_per_sample = usize::from(fmt.bits_per_sample / 8);
    let expected_align = usize::from(fmt.channels) * bytes_per_sample;
    if usize::from(fmt.block_align) != expected_align
        || !data.len().is_multiple_of(bytes_per_sample)
    {
        return Err(Error::new(
            ErrorKind::UnsupportedFormat,
            "inconsistent WAV block alignment",
        )
        .with_path(path));
    }

    let mut total_samples = 0u64;
    let mut sum_square = 0.0f64;
    let mut sample_peak = 0.0f64;
    let mut clip_events = Vec::new();
    let mut clip_start = None::<u64>;
    let mut clip_len = 0u64;

    for chunk in data.chunks_exact(bytes_per_sample) {
        let (sample, is_full_scale) = decode_sample(fmt.bits_per_sample, chunk);
        sample_peak = sample_peak.max(sample.abs());
        sum_square += sample * sample;

        if is_full_scale {
            if clip_start.is_none() {
                clip_start = Some(total_samples);
            }
            clip_len += 1;
        } else if let Some(start_sample) = clip_start.take() {
            clip_events.push(ClipEvent {
                start_sample,
                sample_count: clip_len,
            });
            clip_len = 0;
        }
        total_samples += 1;
    }
    if let Some(start_sample) = clip_start {
        clip_events.push(ClipEvent {
            start_sample,
            sample_count: clip_len,
        });
    }

    let rms = if total_samples == 0 {
        0.0
    } else {
        (sum_square / total_samples as f64).sqrt()
    };
    let frames = total_samples / u64::from(fmt.channels);
    Ok(AudioAnalysis {
        duration_seconds: frames as f64 / f64::from(fmt.sample_rate),
        sample_rate: fmt.sample_rate,
        channels: fmt.channels,
        sample_peak: sample_peak.min(1.0),
        clip_events,
        rms,
        crest_factor: (rms > 0.0).then_some(sample_peak.min(1.0) / rms),
        total_samples,
        provenance: AudioAnalyzerProvenance::default(),
    })
}

fn decode_sample(bits: u16, bytes: &[u8]) -> (f64, bool) {
    match bits {
        8 => {
            let raw = bytes[0];
            let signed = i16::from(raw) - 128;
            (f64::from(signed) / 128.0, raw == 0 || raw == u8::MAX)
        }
        16 => {
            let raw = i16::from_le_bytes(bytes.try_into().expect("16-bit sample"));
            (
                (f64::from(raw) / f64::from(i16::MAX)).clamp(-1.0, 1.0),
                raw == i16::MIN || raw == i16::MAX,
            )
        }
        24 => {
            let raw = i32::from_le_bytes([
                bytes[0],
                bytes[1],
                bytes[2],
                if bytes[2] & 0x80 != 0 { 0xff } else { 0x00 },
            ]);
            let min = -(1_i32 << 23);
            let max = (1_i32 << 23) - 1;
            (
                (raw as f64 / max as f64).clamp(-1.0, 1.0),
                raw == min || raw == max,
            )
        }
        32 => {
            let raw = i32::from_le_bytes(bytes.try_into().expect("32-bit sample"));
            (
                (raw as f64 / i32::MAX as f64).clamp(-1.0, 1.0),
                raw == i32::MIN || raw == i32::MAX,
            )
        }
        _ => unreachable!("validated bit depth"),
    }
}
