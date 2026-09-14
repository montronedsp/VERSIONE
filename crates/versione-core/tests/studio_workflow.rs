//! End-to-end Studio workflow against temporary projects.

use std::fs;

use tempfile::tempdir;
use versione_core::services::{
    focus_board, list_library_cards, pipeline_board, rediscover_candidates,
    skip_rediscover_candidate,
};
use versione_core::{
    add_media_file, add_task, create_checkpoint, find_projects, init_project, keep_version,
    rebuild_library, register_project, set_lifecycle, set_release_stage, update_profile,
    CheckpointOutcome, FindQuery, InitOptions, LifecycleState, MediaKind, NewTask, ReleaseStage,
};

#[test]
fn studio_product_loop_ten_projects() {
    let dir = tempdir().unwrap();
    let lib = dir.path().join("library.toml");
    std::env::set_var("VERSIONE_LIBRARY", &lib);

    let artists = ["Crossing Avenue", "Dorothea", "Andrea"];
    let scales = ["Phrygian", "Dorian", "Minor"];
    let mut roots = Vec::new();

    for i in 0..10 {
        let root = dir.path().join(format!("project-{i:02}"));
        fs::create_dir_all(&root).unwrap();
        init_project(&root, &InitOptions::default()).unwrap();
        update_profile(&root, |p| {
            p.title = Some(format!("Untitled {i:03}"));
            p.artist_aliases = vec![artists[i % artists.len()].into()];
            p.musical.genre = Some(if i % 2 == 0 { "Techno" } else { "Ambient" }.into());
            p.musical.bpm = Some(128.0 + (i as f64));
            p.musical.root_note = Some("F".into());
            p.musical.scale = Some(scales[i % scales.len()].into());
            if i < 4 {
                p.lifecycle = Some(LifecycleState::CreativePool);
            } else if i == 5 {
                p.lifecycle = Some(LifecycleState::Publishing);
            } else {
                p.lifecycle = Some(LifecycleState::Active);
            }
        })
        .unwrap();
        add_task(
            &root,
            NewTask {
                text: format!("Finish arrangement {i}"),
                ..Default::default()
            },
        )
        .unwrap();
        let wav = root.join("preview.wav");
        // Minimal valid-ish RIFF header bytes; media stores opaque bytes.
        fs::write(&wav, b"RIFF....WAVEfmt ").unwrap();
        add_media_file(&root, &wav, MediaKind::AudioPreview, Some("loop".into())).unwrap();
        fs::write(root.join("session.txt"), format!("take-{i}")).unwrap();
        let CheckpointOutcome::Created(_) = create_checkpoint(&root).unwrap() else {
            panic!("checkpoint");
        };
        keep_version(&root, &format!("Structure {i}")).unwrap();
        if i == 5 {
            set_release_stage(&root, ReleaseStage::Mastering).unwrap();
        }
        register_project(&root).unwrap();
        roots.push(root);
    }

    // Rebuild index from scratch.
    let _ = fs::remove_file(&lib);
    for root in &roots {
        register_project(root).unwrap();
    }
    let rebuilt = rebuild_library().unwrap();
    assert_eq!(rebuilt.len(), 10);

    let bpm_hits = find_projects(&FindQuery {
        bpm_min: Some(130.0),
        bpm_max: Some(140.0),
        ..Default::default()
    })
    .unwrap();
    assert!(!bpm_hits.is_empty());

    let phrygian = find_projects(&FindQuery {
        scale: Some("phrygian".into()),
        ..Default::default()
    })
    .unwrap();
    assert!(!phrygian.is_empty());

    let pool = find_projects(&FindQuery {
        lifecycle: Some(LifecycleState::CreativePool),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(pool.len(), 4);

    let artist = find_projects(&FindQuery {
        artist: Some("Crossing Avenue".into()),
        ..Default::default()
    })
    .unwrap();
    assert!(!artist.is_empty());

    let cards = list_library_cards().unwrap();
    assert_eq!(cards.len(), 10);
    assert!(cards.iter().any(|c| c.has_audio_preview));

    let candidates = rediscover_candidates(None, Some(5)).unwrap();
    assert!(!candidates.is_empty());
    skip_rediscover_candidate(&candidates[0].project_id).unwrap();

    let focus = focus_board().unwrap();
    assert!(!focus.open_tasks.is_empty());

    let pipeline = pipeline_board().unwrap();
    assert!(pipeline
        .stages
        .iter()
        .any(|s| s.stage == ReleaseStage::Mastering && !s.projects.is_empty()));

    // Move one Creative Pool project to Active.
    let target = pool[0].root.clone();
    set_lifecycle(&target, LifecycleState::Active).unwrap();
    let still_pool = find_projects(&FindQuery {
        lifecycle: Some(LifecycleState::CreativePool),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(still_pool.len(), 3);

    std::env::remove_var("VERSIONE_LIBRARY");
}
