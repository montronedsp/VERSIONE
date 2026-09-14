//! Deterministic Insights / statistics from stored metadata only.
//!
//! Local, private, inspectable. No telemetry. Missing data stays missing —
//! it is never coerced to zero unless the metric is a count of known items.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::library::list_library_summaries;
use crate::profile::ProjectSummary;
use crate::workflow::LifecycleState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StatsWindow {
    AllTime,
    ThisYear,
    LastYear,
    LastDays(u32),
    Custom {
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    },
}

impl StatsWindow {
    pub fn contains(&self, ts: DateTime<Utc>) -> bool {
        let now = Utc::now();
        match self {
            Self::AllTime => true,
            Self::ThisYear => ts.year() == now.year(),
            Self::LastYear => ts.year() == now.year() - 1,
            Self::LastDays(days) => {
                let cutoff = now - chrono::Duration::days(i64::from(*days));
                ts >= cutoff
            }
            Self::Custom { start, end } => ts >= *start && ts <= *end,
        }
    }
}

use chrono::Datelike;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct CountStat {
    pub label: String,
    pub count: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct LibraryOverview {
    pub projects: u64,
    pub by_lifecycle: Vec<CountStat>,
    pub released: u64,
    pub creative_pool: u64,
    pub active: u64,
    pub median_bpm: Option<f64>,
    pub most_used_root: Option<String>,
    pub most_used_scale: Option<String>,
    pub most_used_genre: Option<String>,
    pub most_used_instrument: Option<String>,
    pub average_channels: Option<f64>,
    pub average_versions: Option<f64>,
    pub unfinished_tasks: u64,
    pub release_completion_ratio: Option<f64>,
}

fn median(mut values: Vec<f64>) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = values.len() / 2;
    if values.len().is_multiple_of(2) {
        Some((values[mid - 1] + values[mid]) / 2.0)
    } else {
        Some(values[mid])
    }
}

fn mode_string(items: impl Iterator<Item = String>) -> Option<String> {
    let mut map = std::collections::HashMap::<String, u64>::new();
    for item in items {
        let key = item.trim().to_string();
        if key.is_empty() {
            continue;
        }
        *map.entry(key).or_default() += 1;
    }
    map.into_iter().max_by_key(|(_, c)| *c).map(|(k, _)| k)
}

fn filter_window(summaries: Vec<ProjectSummary>, window: StatsWindow) -> Vec<ProjectSummary> {
    summaries
        .into_iter()
        .filter(|s| window.contains(s.created_utc) || window.contains(s.updated_utc))
        .collect()
}

pub fn compute_overview(window: StatsWindow) -> Result<LibraryOverview> {
    let summaries = filter_window(list_library_summaries()?, window);
    Ok(overview_from_summaries(&summaries))
}

pub fn overview_from_summaries(summaries: &[ProjectSummary]) -> LibraryOverview {
    let projects = summaries.len() as u64;
    let mut by_lifecycle = Vec::new();
    for state in LifecycleState::all() {
        let count = summaries
            .iter()
            .filter(|s| s.lifecycle == Some(*state))
            .count() as u64;
        if count > 0 {
            by_lifecycle.push(CountStat {
                label: state.as_str().into(),
                count,
            });
        }
    }
    let unset = summaries.iter().filter(|s| s.lifecycle.is_none()).count() as u64;
    if unset > 0 {
        by_lifecycle.push(CountStat {
            label: "unset".into(),
            count: unset,
        });
    }

    let released = summaries
        .iter()
        .filter(|s| s.lifecycle == Some(LifecycleState::Released))
        .count() as u64;
    let creative_pool = summaries
        .iter()
        .filter(|s| s.lifecycle == Some(LifecycleState::CreativePool))
        .count() as u64;
    let active = summaries
        .iter()
        .filter(|s| s.lifecycle == Some(LifecycleState::Active))
        .count() as u64;

    let bpms: Vec<f64> = summaries.iter().filter_map(|s| s.bpm).collect();
    let channels: Vec<f64> = summaries
        .iter()
        .filter_map(|s| s.channel_count.map(f64::from))
        .collect();
    let versions: Vec<f64> = summaries
        .iter()
        .map(|s| f64::from(s.version_count))
        .collect();

    let unfinished_tasks = summaries
        .iter()
        .map(|s| u64::from(s.unfinished_tasks))
        .sum();

    let release_completion_ratio = if projects == 0 {
        None
    } else {
        Some(released as f64 / projects as f64)
    };

    let average_channels = if channels.is_empty() {
        None
    } else {
        Some(channels.iter().sum::<f64>() / channels.len() as f64)
    };
    let average_versions = if versions.is_empty() {
        None
    } else {
        Some(versions.iter().sum::<f64>() / versions.len() as f64)
    };

    LibraryOverview {
        projects,
        by_lifecycle,
        released,
        creative_pool,
        active,
        median_bpm: median(bpms),
        most_used_root: mode_string(summaries.iter().filter_map(|s| s.root_note.clone())),
        most_used_scale: mode_string(summaries.iter().filter_map(|s| s.scale.clone())),
        most_used_genre: mode_string(summaries.iter().filter_map(|s| s.genre.clone())),
        most_used_instrument: mode_string(
            summaries
                .iter()
                .flat_map(|s| s.instrument_names.iter().cloned()),
        ),
        average_channels,
        average_versions,
        unfinished_tasks,
        release_completion_ratio,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct MusicStats {
    pub bpm_distribution: Vec<(String, u64)>,
    pub keys: Vec<CountStat>,
    pub scales: Vec<CountStat>,
    pub genres: Vec<CountStat>,
}

pub fn compute_music_stats(window: StatsWindow) -> Result<MusicStats> {
    let summaries = filter_window(list_library_summaries()?, window);
    Ok(music_from_summaries(&summaries))
}

fn bucket_bpm(bpm: f64) -> String {
    let lo = ((bpm / 10.0).floor() * 10.0) as i64;
    format!("{lo}-{}", lo + 9)
}

fn count_strings(values: impl Iterator<Item = String>) -> Vec<CountStat> {
    let mut map = std::collections::BTreeMap::<String, u64>::new();
    for v in values {
        let key = v.trim().to_string();
        if key.is_empty() {
            continue;
        }
        *map.entry(key).or_default() += 1;
    }
    map.into_iter()
        .map(|(label, count)| CountStat { label, count })
        .collect()
}

pub fn music_from_summaries(summaries: &[ProjectSummary]) -> MusicStats {
    let mut bpm_map = std::collections::BTreeMap::<String, u64>::new();
    for s in summaries {
        if let Some(bpm) = s.bpm {
            *bpm_map.entry(bucket_bpm(bpm)).or_default() += 1;
        }
    }
    MusicStats {
        bpm_distribution: bpm_map.into_iter().collect(),
        keys: count_strings(summaries.iter().filter_map(|s| s.key.clone())),
        scales: count_strings(summaries.iter().filter_map(|s| s.scale.clone())),
        genres: count_strings(summaries.iter().filter_map(|s| s.genre.clone())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::ProjectId;
    use crate::workflow::LifecycleState;
    use std::path::PathBuf;

    fn sample(bpm: Option<f64>, lifecycle: Option<LifecycleState>) -> ProjectSummary {
        ProjectSummary {
            project_id: ProjectId::new(),
            root: PathBuf::from("/tmp/x"),
            title: Some("t".into()),
            aliases: vec![],
            artist_aliases: vec![],
            tags: vec![],
            genre: Some("Techno".into()),
            subgenre: None,
            bpm,
            root_note: Some("F".into()),
            key: Some("F minor".into()),
            scale: Some("Phrygian".into()),
            lifecycle,
            release_stage: None,
            instrument_names: vec!["SWARA XT".into()],
            channel_count: Some(24),
            updated_utc: Utc::now(),
            created_utc: Utc::now(),
            version_count: 3,
            unfinished_tasks: 1,
            has_audio_preview: false,
            has_arrangement_image: false,
        }
    }

    #[test]
    fn overview_ignores_missing_bpm_and_avoids_div_zero() {
        let empty = overview_from_summaries(&[]);
        assert_eq!(empty.projects, 0);
        assert!(empty.median_bpm.is_none());
        assert!(empty.release_completion_ratio.is_none());
        assert!(empty.average_channels.is_none());

        let rows = vec![
            sample(Some(130.0), Some(LifecycleState::Released)),
            sample(None, Some(LifecycleState::CreativePool)),
            sample(Some(140.0), Some(LifecycleState::CreativePool)),
        ];
        let overview = overview_from_summaries(&rows);
        assert_eq!(overview.projects, 3);
        assert_eq!(overview.released, 1);
        assert_eq!(overview.creative_pool, 2);
        assert_eq!(overview.median_bpm, Some(135.0));
        assert_eq!(overview.most_used_root.as_deref(), Some("F"));
        assert!((overview.average_channels.unwrap() - 24.0).abs() < f64::EPSILON);
        assert!((overview.release_completion_ratio.unwrap() - (1.0 / 3.0)).abs() < 1e-9);
    }
}
