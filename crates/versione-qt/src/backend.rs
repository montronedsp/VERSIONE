//! Qt/QML bridge for Studio.

use std::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use versione_core::materialize_media_to_temp;
use versione_core::services::ProjectCard;
use versione_core::LifecycleState;
use versione_studio_model::{LibraryLayout, Nav, SortMode, StudioModel};

use crate::player::PreviewPlayer;

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[namespace = "versione"]
        #[qproperty(QString, status)]
        #[qproperty(QString, nav)]
        #[qproperty(QString, cards_json)]
        #[qproperty(QString, detail_json)]
        #[qproperty(QString, overview_text)]
        #[qproperty(QString, overview_header_json)]
        #[qproperty(QString, focus_json)]
        #[qproperty(QString, pipeline_json)]
        #[qproperty(QString, rediscover_json)]
        #[qproperty(bool, playing)]
        #[qproperty(bool, privacy_ack)]
        #[qproperty(QString, search)]
        #[qproperty(QString, filter_lifecycle)]
        #[qproperty(QString, sort_mode)]
        #[qproperty(QString, layout_mode)]
        type VersioneBackend = super::VersioneBackendRust;

        #[qinvokable]
        fn reload(self: Pin<&mut Self>);

        #[qinvokable]
        #[cxx_name = "navigateTo"]
        fn navigate_to(self: Pin<&mut Self>, nav: &QString);

        #[qinvokable]
        fn select_card(self: Pin<&mut Self>, index: i32);

        #[qinvokable]
        fn save_identity(self: Pin<&mut Self>);

        #[qinvokable]
        fn set_lifecycle(self: Pin<&mut Self>, state: &QString);

        #[qinvokable]
        fn play_selected(self: Pin<&mut Self>);

        #[qinvokable]
        fn play_card(self: Pin<&mut Self>, index: i32);

        #[qinvokable]
        fn pause_audio(self: Pin<&mut Self>);

        #[qinvokable]
        fn resume_audio(self: Pin<&mut Self>);

        #[qinvokable]
        fn stop_audio(self: Pin<&mut Self>);

        #[qinvokable]
        fn add_folder(self: Pin<&mut Self>);

        #[qinvokable]
        fn add_project(self: Pin<&mut Self>);

        #[qinvokable]
        fn rebuild_index(self: Pin<&mut Self>);

        #[qinvokable]
        fn skip_rediscover(self: Pin<&mut Self>);

        #[qinvokable]
        fn next_rediscover(self: Pin<&mut Self>);

        #[qinvokable]
        fn move_rediscover_active(self: Pin<&mut Self>);

        #[qinvokable]
        fn open_pipeline_project(self: Pin<&mut Self>, project_id: &QString);

        #[qinvokable]
        #[cxx_name = "updateSearch"]
        fn update_search(self: Pin<&mut Self>, value: &QString);

        #[qinvokable]
        #[cxx_name = "updateFilterLifecycle"]
        fn update_filter_lifecycle(self: Pin<&mut Self>, value: &QString);

        #[qinvokable]
        #[cxx_name = "updateSortMode"]
        fn update_sort_mode(self: Pin<&mut Self>, value: &QString);

        #[qinvokable]
        #[cxx_name = "updateLayoutMode"]
        fn update_layout_mode(self: Pin<&mut Self>, value: &QString);

        #[qinvokable]
        #[cxx_name = "updateEditTitle"]
        fn update_edit_title(self: Pin<&mut Self>, value: &QString);

        #[qinvokable]
        #[cxx_name = "updateEditBpm"]
        fn update_edit_bpm(self: Pin<&mut Self>, value: &QString);

        #[qinvokable]
        #[cxx_name = "updatePrivacyAck"]
        fn update_privacy_ack(self: Pin<&mut Self>, value: bool);
    }
}

pub struct VersioneBackendRust {
    status: QString,
    nav: QString,
    cards_json: QString,
    detail_json: QString,
    overview_text: QString,
    overview_header_json: QString,
    focus_json: QString,
    pipeline_json: QString,
    rediscover_json: QString,
    playing: bool,
    privacy_ack: bool,
    search: QString,
    filter_lifecycle: QString,
    sort_mode: QString,
    layout_mode: QString,
    model: StudioModel,
    player: Option<PreviewPlayer>,
}

impl Default for VersioneBackendRust {
    fn default() -> Self {
        Self {
            status: QString::from(""),
            nav: QString::from(Nav::Library.label()),
            cards_json: QString::from("[]"),
            detail_json: QString::from("null"),
            overview_text: QString::from(""),
            overview_header_json: QString::from("null"),
            focus_json: QString::from("null"),
            pipeline_json: QString::from("null"),
            rediscover_json: QString::from("null"),
            playing: false,
            privacy_ack: true,
            search: QString::from(""),
            filter_lifecycle: QString::from("any"),
            sort_mode: QString::from("Recent"),
            layout_mode: QString::from("List"),
            model: StudioModel::new(),
            player: PreviewPlayer::new().ok(),
        }
    }
}

fn play_card(model: &mut StudioModel, player: &mut Option<PreviewPlayer>, card: &ProjectCard) {
    let Some(media_id) = &card.audio_preview_media_id else {
        model.status = "No audio preview attached.".into();
        return;
    };
    match materialize_media_to_temp(&card.root_path, &media_id.to_string()) {
        Ok(path) => {
            if player.is_none() {
                *player = PreviewPlayer::new().ok();
            }
            if let Some(player) = player.as_mut() {
                match player.play_file(&path) {
                    Ok(()) => model.status = format!("Playing preview · {}", card.title),
                    Err(err) => model.status = err,
                }
            } else {
                model.status = "Audio output unavailable on this machine.".into();
            }
        }
        Err(err) => model.status = format!("preview: {err}"),
    }
}

struct SyncSnapshot {
    status: QString,
    nav: QString,
    cards_json: QString,
    detail_json: QString,
    overview_text: QString,
    overview_header_json: QString,
    focus_json: QString,
    pipeline_json: QString,
    rediscover_json: QString,
    search: QString,
    filter_lifecycle: QString,
    sort_mode: QString,
    layout_mode: QString,
    privacy_ack: bool,
    playing: bool,
}

fn snapshot(model: &StudioModel, player: &Option<PreviewPlayer>) -> SyncSnapshot {
    SyncSnapshot {
        status: QString::from(&model.status),
        nav: QString::from(model.nav.label()),
        cards_json: QString::from(&model.cards_json_with_index()),
        detail_json: QString::from(
            &serde_json::to_string(&model.project_detail()).unwrap_or_else(|_| "null".into()),
        ),
        overview_text: QString::from(&model.overview_text),
        overview_header_json: QString::from(
            &serde_json::to_string(&model.overview_header()).unwrap_or_else(|_| "null".into()),
        ),
        focus_json: QString::from(
            &serde_json::to_string(&model.focus).unwrap_or_else(|_| "null".into()),
        ),
        pipeline_json: QString::from(
            &serde_json::to_string(&model.pipeline).unwrap_or_else(|_| "null".into()),
        ),
        rediscover_json: QString::from(
            &serde_json::to_string(&model.current_rediscover()).unwrap_or_else(|_| "null".into()),
        ),
        search: QString::from(&model.search),
        filter_lifecycle: QString::from(
            model
                .filter_lifecycle
                .map(|s| s.to_string())
                .unwrap_or_else(|| "any".into()),
        ),
        sort_mode: QString::from(format!("{:?}", model.sort)),
        layout_mode: QString::from(format!("{:?}", model.layout)),
        privacy_ack: model.privacy_ack,
        playing: player.as_ref().map(|p| p.is_playing()).unwrap_or(false),
    }
}

impl qobject::VersioneBackend {
    fn sync_all(mut self: Pin<&mut Self>) {
        let snap = {
            let rust = self.rust();
            snapshot(&rust.model, &rust.player)
        };
        self.as_mut().set_status(snap.status);
        self.as_mut().set_nav(snap.nav);
        self.as_mut().set_cards_json(snap.cards_json);
        self.as_mut().set_detail_json(snap.detail_json);
        self.as_mut().set_overview_text(snap.overview_text);
        self.as_mut()
            .set_overview_header_json(snap.overview_header_json);
        self.as_mut().set_focus_json(snap.focus_json);
        self.as_mut().set_pipeline_json(snap.pipeline_json);
        self.as_mut().set_rediscover_json(snap.rediscover_json);
        self.as_mut().set_search(snap.search);
        self.as_mut().set_filter_lifecycle(snap.filter_lifecycle);
        self.as_mut().set_sort_mode(snap.sort_mode);
        self.as_mut().set_layout_mode(snap.layout_mode);
        self.as_mut().set_privacy_ack(snap.privacy_ack);
        self.as_mut().set_playing(snap.playing);
    }

    pub fn reload(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().model.reload_library();
        self.sync_all();
    }

    pub fn navigate_to(mut self: Pin<&mut Self>, nav: &QString) {
        let label = nav.to_string();
        self.as_mut().rust_mut().model.nav = match label.as_str() {
            "Focus" => Nav::Focus,
            "Rediscover" => Nav::Rediscover,
            "Publishing" => Nav::Publishing,
            "Insights" => Nav::Insights,
            "Settings" => Nav::Settings,
            _ => Nav::Library,
        };
        self.sync_all();
    }

    pub fn select_card(mut self: Pin<&mut Self>, index: i32) {
        if index >= 0 {
            self.as_mut().rust_mut().model.select_card(index as usize);
        }
        self.sync_all();
    }

    pub fn save_identity(mut self: Pin<&mut Self>) {
        if let Err(err) = self.as_mut().rust_mut().model.save_identity() {
            self.as_mut().rust_mut().model.status = err;
        }
        self.sync_all();
    }

    pub fn set_lifecycle(mut self: Pin<&mut Self>, state: &QString) {
        let Some(parsed) = LifecycleState::parse(&state.to_string()) else {
            self.as_mut().rust_mut().model.status = format!("unknown lifecycle: {state}");
            self.sync_all();
            return;
        };
        if let Err(err) = self.as_mut().rust_mut().model.set_project_lifecycle(parsed) {
            self.as_mut().rust_mut().model.status = err;
        }
        self.sync_all();
    }

    pub fn play_selected(mut self: Pin<&mut Self>) {
        let card = self.rust().model.selected_card().cloned();
        if let Some(card) = card {
            let mut rust = self.as_mut().rust_mut();
            let VersioneBackendRust { model, player, .. } = rust.as_mut().get_mut();
            play_card(model, player, &card);
        }
        self.sync_all();
    }

    pub fn play_card(mut self: Pin<&mut Self>, index: i32) {
        let card = if index >= 0 {
            self.rust().model.cards.get(index as usize).cloned()
        } else {
            None
        };
        if let Some(card) = card {
            let mut rust = self.as_mut().rust_mut();
            let VersioneBackendRust { model, player, .. } = rust.as_mut().get_mut();
            play_card(model, player, &card);
        }
        self.sync_all();
    }

    pub fn pause_audio(self: Pin<&mut Self>) {
        if let Some(player) = self.rust().player.as_ref() {
            player.pause();
        }
        self.sync_all();
    }

    pub fn resume_audio(self: Pin<&mut Self>) {
        if let Some(player) = self.rust().player.as_ref() {
            player.resume();
        }
        self.sync_all();
    }

    pub fn stop_audio(mut self: Pin<&mut Self>) {
        if let Some(player) = self.as_mut().rust_mut().player.as_mut() {
            player.stop();
        }
        self.sync_all();
    }

    pub fn add_folder(mut self: Pin<&mut Self>) {
        if let Some(path) = rfd::FileDialog::new().pick_folder() {
            if let Err(err) = self.as_mut().rust_mut().model.add_folder(&path) {
                self.as_mut().rust_mut().model.status = format!("discover: {err}");
            }
        }
        self.sync_all();
    }

    pub fn add_project(mut self: Pin<&mut Self>) {
        if let Some(path) = rfd::FileDialog::new().pick_folder() {
            if let Err(err) = self
                .as_mut()
                .rust_mut()
                .model
                .register_single_project(&path)
            {
                self.as_mut().rust_mut().model.status = err;
            }
        }
        self.sync_all();
    }

    pub fn rebuild_index(mut self: Pin<&mut Self>) {
        if let Err(err) = self.as_mut().rust_mut().model.rebuild_index() {
            self.as_mut().rust_mut().model.status = format!("rebuild: {err}");
        }
        self.sync_all();
    }

    pub fn skip_rediscover(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().model.skip_rediscover();
        self.sync_all();
    }

    pub fn next_rediscover(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().model.next_rediscover();
        self.sync_all();
    }

    pub fn move_rediscover_active(mut self: Pin<&mut Self>) {
        if let Some(candidate) = self.rust().model.current_rediscover().cloned() {
            let path = candidate.card.root_path.clone();
            let _ = self
                .as_mut()
                .rust_mut()
                .model
                .set_lifecycle_for_path(&path, LifecycleState::Active);
        }
        self.sync_all();
    }

    pub fn open_pipeline_project(mut self: Pin<&mut Self>, project_id: &QString) {
        let id_str = project_id.to_string();
        if let Some(idx) = self
            .rust()
            .model
            .cards
            .iter()
            .position(|c| c.project_id.to_string() == id_str)
        {
            self.as_mut().rust_mut().model.select_card(idx);
            self.as_mut().rust_mut().model.nav = Nav::Library;
        }
        self.sync_all();
    }

    pub fn update_search(mut self: Pin<&mut Self>, value: &QString) {
        self.as_mut().rust_mut().model.search = value.to_string();
        self.sync_all();
    }

    pub fn update_filter_lifecycle(mut self: Pin<&mut Self>, value: &QString) {
        let v = value.to_string();
        self.as_mut().rust_mut().model.filter_lifecycle = if v == "any" {
            None
        } else {
            LifecycleState::parse(&v)
        };
        self.sync_all();
    }

    pub fn update_sort_mode(mut self: Pin<&mut Self>, value: &QString) {
        let v = value.to_string();
        self.as_mut().rust_mut().model.sort = match v.as_str() {
            "Oldest" => SortMode::Oldest,
            "Title" => SortMode::Title,
            "Bpm" => SortMode::Bpm,
            "Lifecycle" => SortMode::Lifecycle,
            "Versions" => SortMode::Versions,
            _ => SortMode::Recent,
        };
        self.as_mut().rust_mut().model.apply_sort();
        self.sync_all();
    }

    pub fn update_layout_mode(mut self: Pin<&mut Self>, value: &QString) {
        let v = value.to_string();
        self.as_mut().rust_mut().model.layout = if v == "Grid" {
            LibraryLayout::Grid
        } else {
            LibraryLayout::List
        };
        self.sync_all();
    }

    pub fn update_edit_title(mut self: Pin<&mut Self>, value: &QString) {
        self.as_mut().rust_mut().model.edit_title = value.to_string();
        self.sync_all();
    }

    pub fn update_edit_bpm(mut self: Pin<&mut Self>, value: &QString) {
        self.as_mut().rust_mut().model.edit_bpm = value.to_string();
        self.sync_all();
    }

    pub fn update_privacy_ack(mut self: Pin<&mut Self>, value: bool) {
        self.as_mut().rust_mut().model.privacy_ack = value;
        self.sync_all();
    }
}
