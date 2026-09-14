//! Durable project profile: identity, musical metadata, session hints, workflow.
//!
//! Profile data lives with the project (`.versione/profile.toml`) so it travels
//! with clones/restores. The library index is a rebuildable cache, not authority.

use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{Error, ErrorKind, Result};
use crate::git::GitRepository;
use crate::project::{
    load_config, read_toml, require_initialized, write_toml_atomic, ProjectId, ProjectPaths,
};
use crate::provenance::{Provenance, ProvenancedValue};
use crate::workflow::{LifecycleState, ReleaseInfo};

/// On-disk schema version for `profile.toml` (independent of `config.toml`).
pub const PROFILE_FORMAT_VERSION: u32 = 1;

/// Extensible instrument / sound-source usage record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstrumentUsage {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manufacturer: Option<String>,
    /// Free-form type: synthesizer, drum_machine, plugin, hardware, etc.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

/// Explicit musical metadata. Unconventional scales/descriptions are allowed as strings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct MusicalInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bpm: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bpm_min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bpm_max: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_note: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_signature: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tuning_hz: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub genre: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subgenre: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mood_tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub musical_tags: Vec<String>,
}

/// Session complexity hints. Values may be absent until a user or adapter supplies them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SessionInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel_count: Option<ProvenancedValue<u32>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_track_count: Option<ProvenancedValue<u32>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub midi_track_count: Option<ProvenancedValue<u32>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_count: Option<ProvenancedValue<u32>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub return_count: Option<ProvenancedValue<u32>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub track_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_duration_secs: Option<ProvenancedValue<u64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sample_rate: Option<ProvenancedValue<u32>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bit_depth: Option<ProvenancedValue<u16>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub daw_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub daw_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub daw_provenance: Option<Provenance>,
}

/// Durable creative identity and metadata for one VERSIONE project.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectProfile {
    pub format_version: u32,
    pub project_id: ProjectId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artist_aliases: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub band: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub collaborators: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default)]
    pub musical: MusicalInfo,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub instruments: Vec<InstrumentUsage>,
    #[serde(default)]
    pub session: SessionInfo,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifecycle: Option<LifecycleState>,
    #[serde(default)]
    pub release: ReleaseInfo,
    pub updated_utc: DateTime<Utc>,
}

impl ProjectProfile {
    pub fn new_for(project_id: ProjectId) -> Self {
        Self {
            format_version: PROFILE_FORMAT_VERSION,
            project_id,
            title: None,
            aliases: Vec::new(),
            artist_aliases: Vec::new(),
            band: None,
            collaborators: Vec::new(),
            description: None,
            tags: Vec::new(),
            musical: MusicalInfo::default(),
            instruments: Vec::new(),
            session: SessionInfo::default(),
            lifecycle: None,
            release: ReleaseInfo::default(),
            updated_utc: Utc::now(),
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self.format_version != PROFILE_FORMAT_VERSION {
            return Err(Error::new(
                ErrorKind::UnsupportedFormat,
                format!(
                    "unsupported profile format_version {} (supported {})",
                    self.format_version, PROFILE_FORMAT_VERSION
                ),
            ));
        }
        validate_bpm(self.musical.bpm)?;
        validate_bpm(self.musical.bpm_min)?;
        validate_bpm(self.musical.bpm_max)?;
        if let (Some(min), Some(max)) = (self.musical.bpm_min, self.musical.bpm_max) {
            if min > max {
                return Err(Error::new(
                    ErrorKind::InvalidProject,
                    "bpm_min must be <= bpm_max",
                ));
            }
        }
        if let Some(hz) = self.musical.tuning_hz {
            if !hz.is_finite() || hz <= 0.0 {
                return Err(Error::new(
                    ErrorKind::InvalidProject,
                    "tuning_hz must be a positive finite number",
                ));
            }
        }
        Ok(())
    }
}

fn validate_bpm(bpm: Option<f64>) -> Result<()> {
    let Some(bpm) = bpm else {
        return Ok(());
    };
    if !bpm.is_finite() || bpm <= 0.0 || bpm > 1000.0 {
        return Err(Error::new(
            ErrorKind::InvalidProject,
            "BPM must be a finite number in (0, 1000]",
        ));
    }
    Ok(())
}

impl ProjectPaths {
    pub fn profile_path(&self) -> std::path::PathBuf {
        self.versione_dir.join("profile.toml")
    }
}

/// Load profile or create a default for an existing project (lazy init).
pub fn load_or_default_profile(paths: &ProjectPaths) -> Result<ProjectProfile> {
    if paths.profile_path().exists() {
        return load_profile(paths);
    }
    let config = load_config(paths)?;
    Ok(ProjectProfile::new_for(config.project_id))
}

pub fn load_profile(paths: &ProjectPaths) -> Result<ProjectProfile> {
    let profile: ProjectProfile = read_toml(&paths.profile_path())?;
    profile.validate()?;
    let config = load_config(paths)?;
    if profile.project_id != config.project_id {
        return Err(Error::new(
            ErrorKind::Corruption,
            "profile.toml project_id does not match config.toml",
        )
        .with_path(paths.profile_path()));
    }
    Ok(profile)
}

pub fn save_profile(paths: &ProjectPaths, mut profile: ProjectProfile) -> Result<()> {
    let config = load_config(paths)?;
    profile.project_id = config.project_id;
    profile.format_version = PROFILE_FORMAT_VERSION;
    profile.updated_utc = Utc::now();
    profile.validate()?;
    write_toml_atomic(&paths.profile_path(), &profile)?;
    Ok(())
}

/// Persist profile and create a metadata revision (independent of content checkpoints).
pub fn update_profile(
    root: &Path,
    mut edit: impl FnMut(&mut ProjectProfile),
) -> Result<ProjectProfile> {
    let paths = require_initialized(root)?;
    let mut profile = load_or_default_profile(&paths)?;
    edit(&mut profile);
    save_profile(&paths, profile.clone())?;
    let git = GitRepository::new(root);
    let _ = git.commit_versione_metadata("Update project profile")?;
    Ok(profile)
}

pub fn set_lifecycle(root: &Path, state: LifecycleState) -> Result<ProjectProfile> {
    update_profile(root, |p| {
        p.lifecycle = Some(state);
    })
}

pub fn set_release_stage(
    root: &Path,
    stage: crate::workflow::ReleaseStage,
) -> Result<ProjectProfile> {
    update_profile(root, |p| {
        p.release.stage = Some(stage);
        if p.lifecycle.is_none() {
            p.lifecycle = Some(LifecycleState::Publishing);
        }
    })
}

pub fn ensure_profile_initialized(paths: &ProjectPaths) -> Result<ProjectProfile> {
    if paths.profile_path().exists() {
        return load_profile(paths);
    }
    let profile = load_or_default_profile(paths)?;
    write_toml_atomic(&paths.profile_path(), &profile)?;
    Ok(profile)
}

/// Summary used by dashboards and library indexing (no terminal formatting).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectSummary {
    pub project_id: ProjectId,
    pub root: std::path::PathBuf,
    pub title: Option<String>,
    pub aliases: Vec<String>,
    pub artist_aliases: Vec<String>,
    pub tags: Vec<String>,
    pub genre: Option<String>,
    pub subgenre: Option<String>,
    pub bpm: Option<f64>,
    pub root_note: Option<String>,
    pub key: Option<String>,
    pub scale: Option<String>,
    pub lifecycle: Option<LifecycleState>,
    pub release_stage: Option<crate::workflow::ReleaseStage>,
    pub instrument_names: Vec<String>,
    pub channel_count: Option<u32>,
    pub updated_utc: DateTime<Utc>,
    pub created_utc: DateTime<Utc>,
    pub version_count: u32,
    pub unfinished_tasks: u32,
    pub has_audio_preview: bool,
    pub has_arrangement_image: bool,
}

pub fn project_summary(root: &Path) -> Result<ProjectSummary> {
    let paths = require_initialized(root)?;
    let config = load_config(&paths)?;
    let profile = load_or_default_profile(&paths)?;
    let versions = crate::project::load_version_index(&paths)?;
    let tasks = crate::tasks::load_or_default_tasks(&paths)?;
    let media = crate::media::load_or_default_media(&paths)?;
    Ok(ProjectSummary {
        project_id: config.project_id,
        root: paths.root.clone(),
        title: profile.title.clone(),
        aliases: profile.aliases.clone(),
        artist_aliases: profile.artist_aliases.clone(),
        tags: profile.tags.clone(),
        genre: profile.musical.genre.clone(),
        subgenre: profile.musical.subgenre.clone(),
        bpm: profile.musical.bpm,
        root_note: profile.musical.root_note.clone(),
        key: profile.musical.key.clone(),
        scale: profile.musical.scale.clone(),
        lifecycle: profile.lifecycle,
        release_stage: profile.release.stage,
        instrument_names: profile.instruments.iter().map(|i| i.name.clone()).collect(),
        channel_count: profile.session.channel_count.as_ref().map(|v| v.value),
        updated_utc: profile.updated_utc,
        created_utc: config.created_utc,
        version_count: versions.versions.len() as u32,
        unfinished_tasks: tasks
            .tasks
            .iter()
            .filter(|t| t.status != crate::tasks::TaskStatus::Done)
            .count() as u32,
        has_audio_preview: media
            .items
            .iter()
            .any(|m| m.kind == crate::media::MediaKind::AudioPreview),
        has_arrangement_image: media
            .items
            .iter()
            .any(|m| m.kind == crate::media::MediaKind::ArrangementImage),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{init_project, InitOptions};
    use tempfile::tempdir;

    #[test]
    fn profile_round_trip_and_unicode() {
        let dir = tempdir().unwrap();
        init_project(dir.path(), &InitOptions::default()).unwrap();
        let profile = update_profile(dir.path(), |p| {
            p.title = Some("Dorothéa — Sketch".into());
            p.artist_aliases = vec!["Crossing Avenue".into(), "Andrea".into()];
            p.musical.bpm = Some(135.0);
            p.musical.root_note = Some("F".into());
            p.musical.scale = Some("Phrygian".into());
            p.musical.genre = Some("Techno".into());
            p.tags = vec!["hypnotic".into(), "tribal".into()];
            p.lifecycle = Some(LifecycleState::CreativePool);
            p.instruments.push(InstrumentUsage {
                name: "SWARA XT".into(),
                manufacturer: Some("Custom".into()),
                kind: Some("synthesizer".into()),
                role: Some("lead".into()),
                notes: None,
            });
        })
        .unwrap();
        assert_eq!(profile.musical.bpm, Some(135.0));
        let loaded = load_profile(&ProjectPaths::from_root(dir.path())).unwrap();
        assert_eq!(loaded.title.as_deref(), Some("Dorothéa — Sketch"));
        assert_eq!(loaded.lifecycle, Some(LifecycleState::CreativePool));
    }

    #[test]
    fn invalid_bpm_rejected() {
        let dir = tempdir().unwrap();
        init_project(dir.path(), &InitOptions::default()).unwrap();
        let err = update_profile(dir.path(), |p| {
            p.musical.bpm = Some(-1.0);
        })
        .unwrap_err();
        assert_eq!(err.kind, ErrorKind::InvalidProject);
    }

    #[test]
    fn rename_does_not_change_project_id() {
        let dir = tempdir().unwrap();
        let config = init_project(dir.path(), &InitOptions::default()).unwrap();
        let id = config.project_id.clone();
        update_profile(dir.path(), |p| {
            p.title = Some("Old".into());
        })
        .unwrap();
        update_profile(dir.path(), |p| {
            p.title = Some("New Title".into());
        })
        .unwrap();
        let profile = load_profile(&ProjectPaths::from_root(dir.path())).unwrap();
        assert_eq!(profile.project_id, id);
        assert_eq!(profile.format_version, PROFILE_FORMAT_VERSION);
    }
}
