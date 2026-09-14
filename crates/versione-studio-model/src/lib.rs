//! Shared Studio view-model for the desktop shells.

use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use versione_core::services::{
    add_folder_of_projects, focus_board, list_library_cards, pipeline_board, rediscover_candidates,
    release_checklist, skip_rediscover_candidate, BackupState, DiscoveryOptions, FocusBoard,
    PipelineBoard, ProjectCard, RediscoverCandidate,
};
use versione_core::{
    compute_overview, load_or_default_profile, project_summary, rebuild_library, register_project,
    set_lifecycle, update_profile, LifecycleState, ProjectPaths, StatsWindow,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Nav {
    #[default]
    Library,
    Focus,
    Publishing,
    Insights,
    Rediscover,
    Settings,
}

impl Nav {
    pub const ALL: [Nav; 6] = [
        Nav::Library,
        Nav::Focus,
        Nav::Rediscover,
        Nav::Publishing,
        Nav::Insights,
        Nav::Settings,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Nav::Library => "Library",
            Nav::Focus => "Focus",
            Nav::Rediscover => "Rediscover",
            Nav::Publishing => "Publishing",
            Nav::Insights => "Insights",
            Nav::Settings => "Settings",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LibraryLayout {
    #[default]
    List,
    Grid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum SortMode {
    #[default]
    Recent,
    Oldest,
    Title,
    Bpm,
    Lifecycle,
    Versions,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChecklistItemView {
    pub label: String,
    pub complete: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectDetailView {
    pub title: String,
    pub root_path: String,
    pub edit_title: String,
    pub edit_bpm: String,
    pub has_audio_preview: bool,
    pub version_count: u32,
    pub unfinished_tasks: u32,
    pub checklist: Vec<ChecklistItemView>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverviewHeaderView {
    pub projects: u64,
    pub active: u64,
    pub creative_pool: u64,
    pub publishing: u64,
    pub released: u64,
    pub archived: u64,
}

#[derive(Debug, Clone, Default)]
pub struct StudioModel {
    pub nav: Nav,
    pub status: String,
    pub cards: Vec<ProjectCard>,
    pub selected: Option<usize>,
    pub search: String,
    pub filter_lifecycle: Option<LifecycleState>,
    pub layout: LibraryLayout,
    pub sort: SortMode,
    pub focus: Option<FocusBoard>,
    pub pipeline: Option<PipelineBoard>,
    pub rediscover: Vec<RediscoverCandidate>,
    pub rediscover_idx: usize,
    pub overview_text: String,
    pub edit_title: String,
    pub edit_bpm: String,
    pub privacy_ack: bool,
}

impl StudioModel {
    pub fn new() -> Self {
        let mut model = Self::default();
        model.reload_library();
        model
    }

    pub fn reload_library(&mut self) {
        match list_library_cards() {
            Ok(cards) => {
                self.status = format!("{} projects in library", cards.len());
                self.cards = cards;
                self.apply_sort();
            }
            Err(err) => self.status = format!("library error: {err}"),
        }
        self.focus = focus_board().ok();
        self.pipeline = pipeline_board().ok();
        self.rediscover = rediscover_candidates(None, Some(32)).unwrap_or_default();
        if self.rediscover_idx >= self.rediscover.len() && !self.rediscover.is_empty() {
            self.rediscover_idx = 0;
        }
        if let Ok(overview) = compute_overview(StatsWindow::AllTime) {
            self.overview_text = format!(
                "Projects {p}\nReleased {r}\nCreative Pool {c}\nActive {a}\nMedian BPM {bpm}\nMost used root {root}\nMost used scale {scale}\nMedian channels {ch}\nUnfinished tasks {t}",
                p = overview.projects,
                r = overview.released,
                c = overview.creative_pool,
                a = overview.active,
                bpm = overview
                    .median_bpm
                    .map(|v| format!("{v:.0}"))
                    .unwrap_or_else(|| "—".into()),
                root = overview.most_used_root.as_deref().unwrap_or("—"),
                scale = overview.most_used_scale.as_deref().unwrap_or("—"),
                ch = overview
                    .average_channels
                    .map(|v| format!("{v:.0}"))
                    .unwrap_or_else(|| "—".into()),
                t = overview.unfinished_tasks,
            );
        }
    }

    pub fn apply_sort(&mut self) {
        match self.sort {
            SortMode::Recent => self
                .cards
                .sort_by_key(|b| std::cmp::Reverse(b.last_activity)),
            SortMode::Oldest => self.cards.sort_by_key(|a| a.last_activity),
            SortMode::Title => self.cards.sort_by(|a, b| a.title.cmp(&b.title)),
            SortMode::Bpm => self.cards.sort_by(|a, b| {
                a.bpm
                    .partial_cmp(&b.bpm)
                    .unwrap_or(std::cmp::Ordering::Equal)
            }),
            SortMode::Lifecycle => self
                .cards
                .sort_by(|a, b| format!("{:?}", a.lifecycle).cmp(&format!("{:?}", b.lifecycle))),
            SortMode::Versions => self
                .cards
                .sort_by_key(|b| std::cmp::Reverse(b.version_count)),
        }
    }

    pub fn filtered_indices(&self) -> Vec<usize> {
        let q = self.search.to_lowercase();
        let tokens: Vec<_> = q.split_whitespace().filter(|t| !t.is_empty()).collect();
        self.cards
            .iter()
            .enumerate()
            .filter(|(_, card)| {
                if let Some(life) = self.filter_lifecycle {
                    if card.lifecycle != Some(life) {
                        return false;
                    }
                }
                if tokens.is_empty() {
                    return true;
                }
                let hay = format!(
                    "{} {} {} {} {} {} {} {}",
                    card.title,
                    card.artists.join(" "),
                    card.genre.clone().unwrap_or_default(),
                    card.bpm.map(|b| b.to_string()).unwrap_or_default(),
                    card.root.clone().unwrap_or_default(),
                    card.scale.clone().unwrap_or_default(),
                    card.lifecycle.map(|l| l.to_string()).unwrap_or_default(),
                    card.root_path.display()
                )
                .to_lowercase();
                tokens.iter().all(|t| hay.contains(t))
            })
            .map(|(i, _)| i)
            .collect()
    }

    pub fn select_card(&mut self, idx: usize) {
        self.selected = Some(idx);
        if let Some(card) = self.cards.get(idx) {
            self.edit_title = card.title.clone();
            self.edit_bpm = card.bpm.map(|b| b.to_string()).unwrap_or_default();
        }
    }

    pub fn selected_card(&self) -> Option<&ProjectCard> {
        self.selected.and_then(|idx| self.cards.get(idx))
    }

    pub fn open_project_by_id(&mut self, project_id: &versione_core::project::ProjectId) {
        if let Some(idx) = self.cards.iter().position(|c| &c.project_id == project_id) {
            self.select_card(idx);
            self.nav = Nav::Library;
        }
    }

    pub fn overview_header(&self) -> Option<OverviewHeaderView> {
        let o = compute_overview(StatsWindow::AllTime).ok()?;
        Some(OverviewHeaderView {
            projects: o.projects,
            active: o.active,
            creative_pool: o.creative_pool,
            publishing: o
                .by_lifecycle
                .iter()
                .find(|c| c.label == "publishing")
                .map(|c| c.count)
                .unwrap_or(0),
            released: o.released,
            archived: o
                .by_lifecycle
                .iter()
                .find(|c| c.label == "archived")
                .map(|c| c.count)
                .unwrap_or(0),
        })
    }

    pub fn project_detail(&self) -> Option<ProjectDetailView> {
        let card = self.selected_card()?.clone();
        let (version_count, unfinished_tasks) = project_summary(&card.root_path)
            .map_or((0, 0), |s| (s.version_count, s.unfinished_tasks));
        let checklist = load_or_default_profile(&ProjectPaths::from_root(&card.root_path))
            .map(|profile| {
                release_checklist(&profile.release)
                    .into_iter()
                    .map(|item| ChecklistItemView {
                        label: item.label,
                        complete: item.complete,
                    })
                    .collect()
            })
            .unwrap_or_default();
        Some(ProjectDetailView {
            title: card.title.clone(),
            root_path: card.root_path.display().to_string(),
            edit_title: self.edit_title.clone(),
            edit_bpm: self.edit_bpm.clone(),
            has_audio_preview: card.has_audio_preview,
            version_count,
            unfinished_tasks,
            checklist,
        })
    }

    pub fn save_identity(&mut self) -> Result<(), String> {
        let card = self
            .selected_card()
            .ok_or_else(|| "no project selected".to_string())?
            .clone();
        let bpm = self.edit_bpm.trim().parse::<f64>().ok();
        let title = self.edit_title.clone();
        update_profile(&card.root_path, |p| {
            p.title = Some(title.clone());
            if let Some(bpm) = bpm {
                p.musical.bpm = Some(bpm);
            }
        })
        .map_err(|e| e.to_string())?;
        self.status = "Profile updated.".into();
        self.reload_library();
        Ok(())
    }

    pub fn set_project_lifecycle(&mut self, state: LifecycleState) -> Result<(), String> {
        let card = self
            .selected_card()
            .ok_or_else(|| "no project selected".to_string())?
            .clone();
        set_lifecycle(&card.root_path, state).map_err(|e| e.to_string())?;
        self.status = format!("Lifecycle → {state}");
        self.reload_library();
        Ok(())
    }

    pub fn set_lifecycle_for_path(
        &mut self,
        root_path: &Path,
        state: LifecycleState,
    ) -> Result<(), String> {
        set_lifecycle(root_path, state).map_err(|e| e.to_string())?;
        self.reload_library();
        Ok(())
    }

    pub fn add_folder(&mut self, path: &Path) -> Result<usize, String> {
        let found =
            add_folder_of_projects(path, DiscoveryOptions::default()).map_err(|e| e.to_string())?;
        self.status = format!("Registered {} project(s)", found.len());
        self.reload_library();
        Ok(found.len())
    }

    pub fn register_single_project(&mut self, path: &Path) -> Result<(), String> {
        register_project(path).map_err(|e| e.to_string())?;
        self.status = "Project registered.".into();
        self.reload_library();
        Ok(())
    }

    pub fn rebuild_index(&mut self) -> Result<usize, String> {
        let list = rebuild_library().map_err(|e| e.to_string())?;
        self.status = format!("Rebuilt {} entries", list.len());
        self.reload_library();
        Ok(list.len())
    }

    pub fn skip_rediscover(&mut self) {
        if let Some(candidate) = self.rediscover.get(self.rediscover_idx) {
            let _ = skip_rediscover_candidate(&candidate.project_id);
        }
        self.rediscover_idx = (self.rediscover_idx + 1) % self.rediscover.len().max(1);
        self.reload_library();
    }

    pub fn next_rediscover(&mut self) {
        if !self.rediscover.is_empty() {
            self.rediscover_idx = (self.rediscover_idx + 1) % self.rediscover.len();
        }
    }

    pub fn current_rediscover(&self) -> Option<&RediscoverCandidate> {
        self.rediscover.get(self.rediscover_idx)
    }

    pub fn cards_json(&self) -> String {
        let indices = self.filtered_indices();
        let visible: Vec<_> = indices
            .into_iter()
            .filter_map(|i| self.cards.get(i).cloned())
            .collect();
        serde_json::to_string(&visible).unwrap_or_else(|_| "[]".into())
    }

    pub fn cards_json_with_index(&self) -> String {
        #[derive(Serialize)]
        struct Row {
            index: usize,
            card: ProjectCard,
        }
        let rows: Vec<_> = self
            .filtered_indices()
            .into_iter()
            .map(|index| Row {
                index,
                card: self.cards[index].clone(),
            })
            .collect();
        serde_json::to_string(&rows).unwrap_or_else(|_| "[]".into())
    }
}

pub fn backup_label(state: BackupState) -> &'static str {
    match state {
        BackupState::LocalOnly => "local only",
        BackupState::RemoteConfigured => "remote configured",
        BackupState::RemoteVerified => "remote verified",
        BackupState::Published => "published",
        BackupState::Unknown => "unknown",
    }
}

pub fn format_activity(ts: DateTime<Utc>) -> String {
    ts.to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_nav_is_library() {
        let model = StudioModel::new();
        assert_eq!(model.nav, Nav::Library);
    }
}
