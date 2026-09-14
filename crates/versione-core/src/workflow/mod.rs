//! Creative lifecycle and publishing pipeline state.
//!
//! Lifecycle and release stage are separate fields so a project can be in
//! `publishing` while the release pipeline is at `mastering`.

use serde::{Deserialize, Serialize};

/// Musician-facing creative lifecycle for a project.
///
/// `CreativePool` is first-class: worth keeping, not currently being finished.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum LifecycleState {
    #[default]
    Draft,
    CreativePool,
    Active,
    Publishing,
    Released,
    Archived,
}

impl LifecycleState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::CreativePool => "creative-pool",
            Self::Active => "active",
            Self::Publishing => "publishing",
            Self::Released => "released",
            Self::Archived => "archived",
        }
    }

    pub fn parse(input: &str) -> Option<Self> {
        match normalize_token(input).as_str() {
            "draft" => Some(Self::Draft),
            "creative-pool" | "creative_pool" | "creativepool" => Some(Self::CreativePool),
            "active" => Some(Self::Active),
            "publishing" => Some(Self::Publishing),
            "released" => Some(Self::Released),
            "archived" => Some(Self::Archived),
            _ => None,
        }
    }

    pub fn all() -> &'static [LifecycleState] {
        &[
            Self::Draft,
            Self::CreativePool,
            Self::Active,
            Self::Publishing,
            Self::Released,
            Self::Archived,
        ]
    }
}

impl std::fmt::Display for LifecycleState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Optional release-pipeline stage (orthogonal to [`LifecycleState`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ReleaseStage {
    #[default]
    Candidate,
    Selected,
    Mixing,
    Mastering,
    Artwork,
    Metadata,
    Submitted,
    Scheduled,
    Released,
}

impl ReleaseStage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Candidate => "candidate",
            Self::Selected => "selected",
            Self::Mixing => "mixing",
            Self::Mastering => "mastering",
            Self::Artwork => "artwork",
            Self::Metadata => "metadata",
            Self::Submitted => "submitted",
            Self::Scheduled => "scheduled",
            Self::Released => "released",
        }
    }

    pub fn parse(input: &str) -> Option<Self> {
        match normalize_token(input).as_str() {
            "candidate" => Some(Self::Candidate),
            "selected" => Some(Self::Selected),
            "mixing" => Some(Self::Mixing),
            "mastering" => Some(Self::Mastering),
            "artwork" => Some(Self::Artwork),
            "metadata" => Some(Self::Metadata),
            "submitted" => Some(Self::Submitted),
            "scheduled" => Some(Self::Scheduled),
            "released" => Some(Self::Released),
            _ => None,
        }
    }
}

impl std::fmt::Display for ReleaseStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Publishing / release metadata for a project in (or past) the release pipeline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ReleaseInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage: Option<ReleaseStage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intended_artist: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub track_title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catalog_number: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_format: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_release_date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actual_release_date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub distributor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mastering_status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artwork_status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata_status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub isrc: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upc_ean: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub promo_status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub submission_status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_notes: Option<String>,
}

fn normalize_token(input: &str) -> String {
    input.trim().to_ascii_lowercase().replace(['_', ' '], "-")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_creative_pool_aliases() {
        assert_eq!(
            LifecycleState::parse("creative-pool"),
            Some(LifecycleState::CreativePool)
        );
        assert_eq!(
            LifecycleState::parse("creative_pool"),
            Some(LifecycleState::CreativePool)
        );
    }

    #[test]
    fn rejects_unknown_lifecycle() {
        assert_eq!(LifecycleState::parse("inactive"), None);
    }
}
