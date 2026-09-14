//! VERSIONE Studio application shell (egui).

use std::path::PathBuf;

use eframe::egui::{self, Color32, FontId, RichText, Sense, Vec2};
use versione_core::services::ProjectCard;
use versione_core::{FindQuery, LifecycleState};
use versione_studio_model::{backup_label, LibraryLayout, Nav, SortMode, StudioModel};

use crate::player::PreviewPlayer;

pub struct VersioneApp {
    model: StudioModel,
    player: Option<PreviewPlayer>,
}

impl VersioneApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut style = (*cc.egui_ctx.style()).clone();
        style.visuals.dark_mode = false;
        style.visuals.panel_fill = Color32::from_rgb(246, 244, 239);
        style.visuals.window_fill = Color32::from_rgb(250, 248, 243);
        style.visuals.override_text_color = Some(Color32::from_rgb(28, 28, 26));
        style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(232, 228, 220);
        style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(220, 214, 204);
        style.visuals.selection.bg_fill = Color32::from_rgb(60, 70, 68);
        cc.egui_ctx.set_style(style);

        Self {
            model: StudioModel::new(),
            player: PreviewPlayer::new().ok(),
        }
    }

    fn play_preview(&mut self, card: &ProjectCard) {
        use versione_core::materialize_media_to_temp;
        let Some(media_id) = &card.audio_preview_media_id else {
            self.model.status = "No audio preview attached.".into();
            return;
        };
        match materialize_media_to_temp(&card.root_path, &media_id.to_string()) {
            Ok(path) => {
                if self.player.is_none() {
                    self.player = PreviewPlayer::new().ok();
                }
                if let Some(player) = self.player.as_mut() {
                    match player.play_file(&path) {
                        Ok(()) => self.model.status = format!("Playing preview · {}", card.title),
                        Err(err) => self.model.status = err,
                    }
                } else {
                    self.model.status = "Audio output unavailable on this machine.".into();
                }
            }
            Err(err) => self.model.status = format!("preview: {err}"),
        }
    }
}

impl eframe::App for VersioneApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("VERSIONE")
                        .font(FontId::proportional(28.0))
                        .strong(),
                );
                ui.label(
                    RichText::new("  creative library")
                        .font(FontId::proportional(16.0))
                        .color(Color32::from_rgb(90, 90, 86)),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Reload").clicked() {
                        self.model.reload_library();
                    }
                });
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                for nav in Nav::ALL {
                    let selected = self.model.nav == nav;
                    if ui
                        .selectable_label(selected, RichText::new(nav.label()).size(15.0))
                        .clicked()
                    {
                        self.model.nav = nav;
                    }
                    ui.add_space(8.0);
                }
            });
            ui.add_space(6.0);
            ui.separator();
        });

        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(&self.model.status);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let playing = self
                        .player
                        .as_ref()
                        .map(|p| p.is_playing())
                        .unwrap_or(false);
                    if playing {
                        if ui.button("Pause").clicked() {
                            if let Some(p) = self.player.as_ref() {
                                p.pause();
                            }
                        }
                    } else if ui.button("Resume").clicked() {
                        if let Some(p) = self.player.as_ref() {
                            p.resume();
                        }
                    }
                    if ui.button("Stop").clicked() {
                        if let Some(p) = self.player.as_mut() {
                            p.stop();
                        }
                    }
                });
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| match self.model.nav {
            Nav::Library => self.ui_library(ui),
            Nav::Focus => self.ui_focus(ui),
            Nav::Publishing => self.ui_publishing(ui),
            Nav::Insights => self.ui_insights(ui),
            Nav::Rediscover => self.ui_rediscover(ui),
            Nav::Settings => self.ui_settings(ui),
        });
    }
}

impl VersioneApp {
    fn ui_library(&mut self, ui: &mut egui::Ui) {
        if let Some(o) = self.model.overview_header() {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("{} Projects", o.projects))
                        .strong()
                        .size(22.0),
                );
                ui.label(format!(
                    "  {} Active · {} Creative Pool · {} Publishing · {} Released · {} Archived",
                    o.active, o.creative_pool, o.publishing, o.released, o.archived
                ));
            });
            ui.add_space(8.0);
        }

        ui.horizontal(|ui| {
            ui.label("Search");
            ui.add(
                egui::TextEdit::singleline(&mut self.model.search)
                    .desired_width(320.0)
                    .hint_text("135 F phrygian Crossing Avenue"),
            );
            egui::ComboBox::from_label("Lifecycle")
                .selected_text(
                    self.model
                        .filter_lifecycle
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| "any".into()),
                )
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_label(self.model.filter_lifecycle.is_none(), "any")
                        .clicked()
                    {
                        self.model.filter_lifecycle = None;
                    }
                    for state in LifecycleState::all() {
                        if ui
                            .selectable_label(
                                self.model.filter_lifecycle == Some(*state),
                                state.as_str(),
                            )
                            .clicked()
                        {
                            self.model.filter_lifecycle = Some(*state);
                        }
                    }
                });
            egui::ComboBox::from_label("Sort")
                .selected_text(format!("{:?}", self.model.sort))
                .show_ui(ui, |ui| {
                    for mode in [
                        SortMode::Recent,
                        SortMode::Oldest,
                        SortMode::Title,
                        SortMode::Bpm,
                        SortMode::Lifecycle,
                        SortMode::Versions,
                    ] {
                        if ui
                            .selectable_label(self.model.sort == mode, format!("{mode:?}"))
                            .clicked()
                        {
                            self.model.sort = mode;
                            self.model.apply_sort();
                        }
                    }
                });
            if ui
                .selectable_label(self.model.layout == LibraryLayout::List, "List")
                .clicked()
            {
                self.model.layout = LibraryLayout::List;
            }
            if ui
                .selectable_label(self.model.layout == LibraryLayout::Grid, "Grid")
                .clicked()
            {
                self.model.layout = LibraryLayout::Grid;
            }
            if ui.button("Add folder…").clicked() {
                if let Some(path) = rfd::FileDialog::new().pick_folder() {
                    if let Err(err) = self.model.add_folder(&path) {
                        self.model.status = format!("discover: {err}");
                    }
                }
            }
        });
        ui.add_space(8.0);

        let indices = self.model.filtered_indices();
        egui::SidePanel::right("project_detail")
            .default_width(380.0)
            .show_inside(ui, |ui| {
                self.ui_project_detail(ui);
            });

        egui::ScrollArea::vertical().show(ui, |ui| {
            if self.model.layout == LibraryLayout::List {
                for idx in indices {
                    let card = self.model.cards[idx].clone();
                    let selected = self.model.selected == Some(idx);
                    let response = ui.add_sized(
                        Vec2::new(ui.available_width(), 72.0),
                        egui::Button::new("").fill(if selected {
                            Color32::from_rgb(228, 232, 228)
                        } else {
                            Color32::TRANSPARENT
                        }),
                    );
                    let rect = response.rect;
                    ui.allocate_new_ui(egui::UiBuilder::new().max_rect(rect), |ui| {
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.label(RichText::new(&card.title).strong().size(16.0));
                                ui.label(format!(
                                    "{} · {} · {} {}",
                                    card.artists.first().cloned().unwrap_or_default(),
                                    card.genre.clone().unwrap_or_else(|| "—".into()),
                                    card.bpm
                                        .map(|b| format!("{b:.0} BPM"))
                                        .unwrap_or_else(|| "—".into()),
                                    match (&card.root, &card.scale) {
                                        (Some(r), Some(s)) => format!("{r} {s}"),
                                        _ => String::new(),
                                    }
                                ));
                                ui.label(format!(
                                    "{} · V{:02} · {} open tasks · {}",
                                    card.lifecycle
                                        .map(|l| l.to_string())
                                        .unwrap_or_else(|| "unset".into()),
                                    card.version_count,
                                    card.unfinished_tasks,
                                    backup_label(card.backup_state),
                                ));
                            });
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if card.has_audio_preview && ui.button("Play").clicked() {
                                        self.play_preview(&card);
                                    }
                                },
                            );
                        });
                    });
                    if response.clicked() {
                        self.model.select_card(idx);
                    }
                    ui.separator();
                }
            } else {
                ui.horizontal_wrapped(|ui| {
                    for idx in indices {
                        let card = self.model.cards[idx].clone();
                        let selected = self.model.selected == Some(idx);
                        let (rect, response) =
                            ui.allocate_exact_size(Vec2::new(220.0, 140.0), Sense::click());
                        let fill = if selected {
                            Color32::from_rgb(228, 232, 228)
                        } else {
                            Color32::from_rgb(238, 234, 226)
                        };
                        ui.painter().rect_filled(rect, 2.0, fill);
                        ui.allocate_new_ui(
                            egui::UiBuilder::new().max_rect(rect.shrink(10.0)),
                            |ui| {
                                ui.label(RichText::new(&card.title).strong());
                                ui.label(
                                    card.lifecycle
                                        .map(|l| l.to_string())
                                        .unwrap_or_else(|| "unset".into()),
                                );
                                ui.label(
                                    card.bpm
                                        .map(|b| format!("{b:.0} BPM"))
                                        .unwrap_or_else(|| "—".into()),
                                );
                                if card.has_audio_preview && ui.button("Play").clicked() {
                                    self.play_preview(&card);
                                }
                            },
                        );
                        if response.clicked() {
                            self.model.select_card(idx);
                        }
                    }
                });
            }
        });
    }

    fn ui_project_detail(&mut self, ui: &mut egui::Ui) {
        let Some(detail) = self.model.project_detail() else {
            ui.label("Select a project from the library.");
            return;
        };
        ui.label(RichText::new(&detail.title).strong().size(20.0));
        ui.label(&detail.root_path);
        ui.add_space(8.0);
        ui.label("Title");
        ui.text_edit_singleline(&mut self.model.edit_title);
        ui.label("BPM");
        ui.text_edit_singleline(&mut self.model.edit_bpm);
        if ui.button("Save identity").clicked() {
            if let Err(err) = self.model.save_identity() {
                self.model.status = err;
            }
        }
        ui.add_space(8.0);
        if let Some(card) = self.model.selected_card().cloned() {
            ui.horizontal(|ui| {
                for state in LifecycleState::all() {
                    if ui.button(state.as_str()).clicked() {
                        if let Err(err) = self.model.set_project_lifecycle(*state) {
                            self.model.status = err;
                        }
                    }
                }
            });
            ui.add_space(8.0);
            if card.has_audio_preview && ui.button("Play preview").clicked() {
                self.play_preview(&card);
            }
        }
        ui.label(format!(
            "Versions: {} · Tasks open: {}",
            detail.version_count, detail.unfinished_tasks
        ));
        ui.add_space(6.0);
        ui.label(RichText::new("Release checklist").strong());
        for item in &detail.checklist {
            ui.label(format!(
                "{} {}",
                if item.complete { "[x]" } else { "[ ]" },
                item.label
            ));
        }
    }

    fn ui_focus(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Focus").strong().size(22.0));
        ui.label("Evidence-based queues for finishing work already in the library.");
        ui.add_space(8.0);
        let Some(board) = self.model.focus.clone() else {
            ui.label("Focus board unavailable.");
            return;
        };
        section(ui, "Active", &board.active);
        section(ui, "Needs attention", &board.needs_attention);
        section(ui, "Almost finished", &board.almost_finished);
        section(ui, "Dormant", &board.dormant);
        section(ui, "Publishing candidates", &board.publishing_candidates);
        ui.add_space(8.0);
        ui.label(RichText::new("Open tasks").strong());
        for task in &board.open_tasks {
            ui.label(format!(
                "{} — {} · {}",
                task.project_title,
                task.task.text,
                task.root_path.display()
            ));
        }
    }

    fn ui_publishing(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Publishing").strong().size(22.0));
        let Some(board) = self.model.pipeline.clone() else {
            ui.label("Publishing board unavailable.");
            return;
        };
        ui.horizontal_wrapped(|ui| {
            for col in &board.stages {
                ui.group(|ui| {
                    ui.set_min_width(140.0);
                    ui.label(RichText::new(col.stage.as_str()).strong());
                    for card in &col.projects {
                        ui.label(&card.title);
                        if ui.small_button("Open").clicked() {
                            self.model.open_project_by_id(&card.project_id);
                        }
                    }
                });
            }
        });
    }

    fn ui_insights(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Insights").strong().size(22.0));
        ui.label("Local, private, factual. No telemetry. No gamification.");
        ui.add_space(8.0);
        ui.monospace(&self.model.overview_text);
        if let Ok(o) = versione_core::compute_overview(versione_core::StatsWindow::AllTime) {
            ui.add_space(12.0);
            ui.label(RichText::new("Creative fingerprint").strong());
            ui.label(format!(
                "{} projects · median BPM {} · root {} · scale {}",
                o.projects,
                o.median_bpm
                    .map(|v| format!("{v:.0}"))
                    .unwrap_or_else(|| "—".into()),
                o.most_used_root.as_deref().unwrap_or("—"),
                o.most_used_scale.as_deref().unwrap_or("—"),
            ));
            if let Some(inst) = &o.most_used_instrument {
                ui.label(format!("Most used instrument: {inst}"));
            }
        }
        let _ = FindQuery::default();
    }

    fn ui_rediscover(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Rediscover").strong().size(22.0));
        ui.label("One project at a time. Skip never destroys or downgrades a project.");
        ui.add_space(8.0);
        if self.model.rediscover.is_empty() {
            ui.label("No rediscovery candidates from the current library.");
            return;
        }
        let candidate = self.model.rediscover[self.model.rediscover_idx].clone();
        ui.label(RichText::new(&candidate.card.title).strong().size(20.0));
        ui.label(candidate.card.artists.join(", "));
        ui.label(format!(
            "{} · {} {}",
            candidate
                .card
                .bpm
                .map(|b| format!("{b:.0} BPM"))
                .unwrap_or_else(|| "—".into()),
            candidate.card.root.clone().unwrap_or_default(),
            candidate.card.scale.clone().unwrap_or_default()
        ));
        ui.label(format!(
            "{} versions · last activity {}",
            candidate.card.version_count,
            candidate.card.last_activity.to_rfc3339()
        ));
        ui.add_space(6.0);
        ui.label(RichText::new("Rediscovered because:").strong());
        for reason in &candidate.reasons {
            ui.label(format!("· {}", reason.explanation));
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if candidate.card.has_audio_preview && ui.button("Play").clicked() {
                self.play_preview(&candidate.card);
            }
            if ui.button("Move to Active").clicked() {
                let path = candidate.card.root_path.clone();
                let _ = self
                    .model
                    .set_lifecycle_for_path(&path, LifecycleState::Active);
            }
            if ui.button("Skip").clicked() {
                self.model.skip_rediscover();
            }
            if ui.button("Next").clicked() {
                self.model.next_rediscover();
            }
        });
    }

    fn ui_settings(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Settings").strong().size(22.0));
        ui.add_space(8.0);
        ui.group(|ui| {
            ui.label(RichText::new("Privacy").strong());
            ui.label(
                "Your project library and Insights stay on this computer.\n\
VERSIONE does not send analytics or project metadata to a VERSIONE service.\n\
Remote project content is uploaded only to storage you configure.",
            );
            ui.checkbox(
                &mut self.model.privacy_ack,
                "I understand VERSIONE is local-first",
            );
        });
        ui.add_space(8.0);
        ui.label("Library index: ~/.versione/library.toml (or VERSIONE_LIBRARY)");
        ui.label("Credentials for remote storage come from environment variables only.");
        if ui.button("Rebuild library index").clicked() {
            if let Err(err) = self.model.rebuild_index() {
                self.model.status = format!("rebuild: {err}");
            }
        }
        if ui.button("Add project…").clicked() {
            if let Some(path) = rfd::FileDialog::new().pick_folder() {
                if let Err(err) = self.model.register_single_project(&path) {
                    self.model.status = err;
                }
            }
        }
        let _ = PathBuf::new();
    }
}

fn section(ui: &mut egui::Ui, title: &str, cards: &[ProjectCard]) {
    ui.add_space(6.0);
    ui.label(RichText::new(format!("{title} ({})", cards.len())).strong());
    for card in cards.iter().take(12) {
        ui.label(format!(
            "  {} · {} · {}",
            card.title,
            card.lifecycle
                .map(|l| l.to_string())
                .unwrap_or_else(|| "unset".into()),
            card.bpm
                .map(|b| format!("{b:.0} BPM"))
                .unwrap_or_else(|| "—".into())
        ));
    }
}
