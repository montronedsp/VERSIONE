//! Development CLI for VERSIONE. Not the final musician UX.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use versione_core::{
    add_media_file, add_note, add_screenshot, add_task, complete_task, compute_music_stats,
    compute_overview, configure_remote_storage, create_checkpoint, enable_project, find_projects,
    hash_file, init_project, keep_version, list_history, list_notes, list_screenshots, list_tasks,
    load_or_default_profile, project_summary, publish_project, rebuild_library, rediscovery_hints,
    register_project, restore_version_as_copy, rich_status, set_lifecycle, set_release_stage,
    show_note, snapshot_project, update_profile, verify_working_tree, AddScreenshotOptions,
    CheckpointOutcome, EnableOptions, FindQuery, HistoryEntryKind, InitOptions, LifecycleState,
    MediaKind, NewTask, ProjectPaths, PublishOptions, ReleaseStage, RemoteStorageConfig,
    StatsWindow, TaskPriority,
};

#[derive(Debug, Parser)]
#[command(
    name = "versione",
    version,
    about = "VERSIONE — creative memory and version control for music projects"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Initialize VERSIONE metadata beside a music project directory.
    Init {
        /// Path to the music project root directory.
        project: PathBuf,
    },
    /// Enable VERSIONE on a music project (collect → verify → first snapshot).
    Enable {
        /// Project directory or `.als` file path.
        project: PathBuf,
        /// Explicit external media files to copy into `Media/imported/`.
        #[arg(long = "collect")]
        collect: Vec<PathBuf>,
    },
    /// Show project, storage, and Git status.
    Status {
        /// Path to the music project root directory.
        project: PathBuf,
    },
    /// Verify working-tree integrity (existence, size, BLAKE3 readability).
    Verify {
        /// Path to the music project root directory.
        project: PathBuf,
    },
    /// Create a checkpoint of the current project tree.
    Checkpoint {
        /// Path to the music project root directory.
        project: PathBuf,
    },
    /// Alias for checkpoint: verified snapshot of the current project tree.
    Snapshot {
        /// Path to the music project root directory.
        project: PathBuf,
    },
    /// Keep a named Version of the current project state.
    Version {
        /// Musician-facing Version name.
        name: String,
        /// Path to the music project root directory.
        project: PathBuf,
    },
    /// List Versions (newest first).
    History {
        /// Path to the music project root directory.
        project: PathBuf,
        /// Include latest checkpoint tip when it is not a Version.
        #[arg(long)]
        include_checkpoint: bool,
        /// Show internal snapshot identifiers.
        #[arg(long, short = 'v')]
        verbose: bool,
    },
    /// Restore a Version into a new destination directory (open as copy).
    Restore {
        /// Version selector: V01, name, or snapshot id.
        version: String,
        /// Empty destination directory for the restored copy.
        #[arg(long = "to")]
        to: PathBuf,
        /// Path to the music project root directory.
        project: PathBuf,
    },
    /// Upload objects to remote storage, then push Git metadata if verified.
    Publish {
        /// Path to the music project root directory.
        project: PathBuf,
        /// Optional Version selector (defaults to current Version/checkpoint).
        #[arg(long)]
        version: Option<String>,
    },
    /// Configure storage backends (no credentials; use environment variables).
    Storage {
        #[command(subcommand)]
        command: StorageCommands,
    },
    /// Project profile and identity.
    Project {
        #[command(subcommand)]
        command: ProjectCommands,
    },
    /// Creative lifecycle state.
    Lifecycle {
        #[command(subcommand)]
        command: LifecycleCommands,
    },
    /// Publishing / release pipeline.
    Release {
        #[command(subcommand)]
        command: ReleaseCommands,
    },
    /// Project notes.
    Note {
        #[command(subcommand)]
        command: NoteCommands,
    },
    /// Project tasks.
    Task {
        #[command(subcommand)]
        command: TaskCommands,
    },
    /// Preview media (object-store backed).
    Preview {
        #[command(subcommand)]
        command: PreviewCommands,
    },
    /// Optional snapshot screenshots (visual project memory; not required for reconstruction).
    Screenshot {
        #[command(subcommand)]
        command: ScreenshotCommands,
    },
    /// Local project library index.
    Library {
        #[command(subcommand)]
        command: LibraryCommands,
    },
    /// Search registered projects (metadata only; no audio hashing).
    Find {
        #[arg(long)]
        genre: Option<String>,
        #[arg(long)]
        bpm: Option<String>,
        #[arg(long)]
        scale: Option<String>,
        #[arg(long)]
        key: Option<String>,
        #[arg(long)]
        artist: Option<String>,
        #[arg(long)]
        instrument: Option<String>,
        #[arg(long)]
        tag: Option<String>,
        #[arg(long)]
        state: Option<String>,
        #[arg(long)]
        name: Option<String>,
        /// Inactive for N days (e.g. 180 or 180d).
        #[arg(long = "inactive-for")]
        inactive_for: Option<String>,
        #[arg(long)]
        unfinished: bool,
    },
    /// Evidence-based rediscovery hints (no judgment language).
    Rediscover,
    /// Deterministic library statistics from stored metadata.
    Stats {
        #[command(subcommand)]
        command: Option<StatsCommands>,
    },
    /// Stream-hash a file with BLAKE3 (debugging aid).
    HashFile {
        /// File to hash.
        path: PathBuf,
    },
}

#[derive(Debug, Subcommand)]
enum StorageCommands {
    /// Configure an S3-compatible remote object store (non-secret settings only).
    SetRemote {
        project: PathBuf,
        #[arg(long)]
        bucket: String,
        #[arg(long)]
        region: String,
        #[arg(long)]
        endpoint: Option<String>,
        #[arg(long, default_value = "versione/")]
        prefix: String,
        #[arg(long, default_value = "remote")]
        location_id: String,
    },
}

#[derive(Debug, Subcommand)]
#[allow(clippy::large_enum_variant)]
enum ProjectCommands {
    /// Show project summary / profile.
    Show { project: PathBuf },
    /// Edit common profile fields.
    Edit {
        project: PathBuf,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        artist: Option<String>,
        #[arg(long)]
        genre: Option<String>,
        #[arg(long)]
        subgenre: Option<String>,
        #[arg(long)]
        bpm: Option<f64>,
        #[arg(long)]
        root: Option<String>,
        #[arg(long)]
        scale: Option<String>,
        #[arg(long)]
        key: Option<String>,
        #[arg(long)]
        tag: Vec<String>,
        #[arg(long)]
        instrument: Vec<String>,
        #[arg(long)]
        description: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
enum LifecycleCommands {
    /// Set creative lifecycle (draft, creative-pool, active, publishing, released, archived).
    Set { state: String, project: PathBuf },
}

#[derive(Debug, Subcommand)]
enum ReleaseCommands {
    /// Show release pipeline fields.
    Status { project: PathBuf },
    /// Set release stage (candidate, mixing, mastering, …).
    SetStage { stage: String, project: PathBuf },
}

#[derive(Debug, Subcommand)]
enum NoteCommands {
    Add {
        project: PathBuf,
        text: String,
        #[arg(long)]
        category: Option<String>,
    },
    List {
        project: PathBuf,
    },
    Show {
        project: PathBuf,
        id: String,
    },
}

#[derive(Debug, Subcommand)]
enum TaskCommands {
    Add {
        project: PathBuf,
        text: String,
        #[arg(long)]
        priority: Option<String>,
        #[arg(long)]
        category: Option<String>,
    },
    List {
        project: PathBuf,
    },
    Done {
        project: PathBuf,
        id: String,
    },
}

#[derive(Debug, Subcommand)]
enum PreviewCommands {
    Audio {
        #[command(subcommand)]
        command: PreviewAddCommands,
    },
    Image {
        #[command(subcommand)]
        command: PreviewAddCommands,
    },
}

#[derive(Debug, Subcommand)]
enum PreviewAddCommands {
    Add {
        file: PathBuf,
        project: PathBuf,
        #[arg(long)]
        label: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
enum ScreenshotCommands {
    /// Import an image file as optional visual memory for the current (or given) snapshot.
    Add {
        /// Image file to import (png/jpg/webp recommended).
        file: PathBuf,
        /// Path to the music project root.
        project: PathBuf,
        #[arg(long)]
        label: Option<String>,
        /// Optional snapshot id (defaults to current checkpoint tip).
        #[arg(long)]
        snapshot: Option<String>,
    },
    /// List imported screenshots for a project.
    List { project: PathBuf },
}

#[derive(Debug, Subcommand)]
enum LibraryCommands {
    /// Register a project root in the local rebuildable index.
    Add { project: PathBuf },
    /// Rebuild the index from registered roots.
    Rebuild,
}

#[derive(Debug, Subcommand)]
enum StatsCommands {
    Projects,
    Music,
    Workflow,
    Instruments,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}

fn parse_bpm_range(spec: &str) -> Result<(Option<f64>, Option<f64>), Box<dyn std::error::Error>> {
    if let Some((a, b)) = spec.split_once("..") {
        let min = if a.trim().is_empty() {
            None
        } else {
            Some(a.trim().parse()?)
        };
        let max = if b.trim().is_empty() {
            None
        } else {
            Some(b.trim().parse()?)
        };
        Ok((min, max))
    } else {
        let v: f64 = spec.trim().parse()?;
        Ok((Some(v), Some(v)))
    }
}

fn parse_inactive_days(spec: &str) -> Result<u64, Box<dyn std::error::Error>> {
    let s = spec.trim().to_ascii_lowercase();
    let s = s.strip_suffix('d').unwrap_or(&s);
    Ok(s.parse()?)
}

fn print_checkpoint(outcome: CheckpointOutcome) -> Result<(), Box<dyn std::error::Error>> {
    match outcome {
        CheckpointOutcome::Created(report) => {
            println!(
                "Created checkpoint {}",
                &report.snapshot_id.to_string()[..8]
            );
            println!("{} project files", report.file_count);
            println!("{} large objects", report.large_object_count);
            println!("{} new objects", report.new_objects);
            println!("{} reused objects", report.reused_objects);
        }
        CheckpointOutcome::Unchanged { snapshot_id, .. } => {
            println!("No project changes detected.");
            println!(
                "Latest checkpoint is already current ({}).",
                &snapshot_id.to_string()[..8]
            );
        }
    }
    Ok(())
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Init { project } => {
            let config = init_project(&project, &InitOptions::default())?;
            let _ = register_project(&project);
            println!("Initialized VERSIONE project");
            println!("project_id: {}", config.project_id);
            println!("format_version: {}", config.format_version);
            println!("hash_algorithm: {}", config.hash_algorithm);
            println!("Git: local repository ready");
            println!("Object storage: local (.versione/objects)");
        }
        Commands::Enable { project, collect } => {
            let report = enable_project(
                &project,
                &EnableOptions {
                    external_paths: collect,
                },
            )?;
            let _ = register_project(&report.root);
            println!("Project detected ({})", report.kind);
            println!("Project: {}", report.root.display());
            println!("Project id: {}", report.project_id);
            if let Some(pf) = &report.primary_project_file {
                println!("Project file: {}", pf.display());
            }
            if report.daw_references.total > 0 {
                println!(
                    "Media references: {} (local {}, external {}, missing {})",
                    report.daw_references.total,
                    report.daw_references.project_local,
                    report.daw_references.external,
                    report.daw_references.missing
                );
            }
            if report.already_enabled {
                println!("VERSIONE already enabled — history preserved");
            } else {
                if report.collection.copied > 0 {
                    println!(
                        "Media collected: {} file(s) → Media/imported/",
                        report.collection.copied
                    );
                } else {
                    println!("Media collected: none (project assets left in place)");
                }
                if let Some(true) = report.original_rpp_unchanged {
                    println!("Canonical .rpp: unchanged (snapshot uses staged rewrite)");
                }
                for note in &report.collection.unsupported_notes {
                    println!("Note: {note}");
                }
                println!(
                    "Manifest / integrity: {} object(s) verified",
                    report.verification.verified
                );
                match &report.checkpoint {
                    Some(CheckpointOutcome::Created(c)) => {
                        println!(
                            "Initial snapshot created: {} ({} files, {} new objects, {} reused)",
                            &c.snapshot_id.to_string()[..8],
                            c.file_count,
                            c.new_objects,
                            c.reused_objects
                        );
                    }
                    Some(CheckpointOutcome::Unchanged { snapshot_id, .. }) => {
                        println!(
                            "Initial snapshot unchanged: {}",
                            &snapshot_id.to_string()[..8]
                        );
                    }
                    None => {}
                }
            }
            println!("Lifecycle: {}", report.lifecycle);
        }
        Commands::Status { project } => {
            let status = rich_status(&project)?;
            if !status.initialized {
                println!("VERSIONE");
                println!();
                println!("State: not initialized (unmanaged)");
                println!("Project: {}", status.root.display());
                println!();
                println!("Run: versione enable <project>");
                return Ok(());
            }

            println!("VERSIONE");
            println!();
            println!("Enablement: {}", status.enablement);
            if let Some(kind) = status.project_kind {
                println!("DAW: {kind}");
            }
            if let Some(file) = &status.project_file {
                println!("Project file: {file}");
            }
            if let Some(id) = &status.project_id {
                println!("Project id: {id}");
            }
            println!("Project: {}", status.root.display());
            if let Some(refs) = &status.daw_references {
                if refs.total > 0 {
                    println!("Media references: {}", refs.total);
                    println!("  project-local: {}", refs.project_local);
                    println!("  external: {}", refs.external);
                    println!("  collected (last enable): {}", refs.collected);
                    println!("  missing: {}", refs.missing);
                    println!(
                        "  unresolved/unsafe/unsupported: {}",
                        refs.unresolved + refs.unsafe_path + refs.unsupported
                    );
                }
            }
            if let Some(note) = &status.self_contained_note {
                println!("Collection: {note}");
            }
            if !status.unresolved_external_notes.is_empty() {
                println!("Notes:");
                for n in &status.unresolved_external_notes {
                    println!("  - {n}");
                }
            }
            if let Ok(summary) = project_summary(&project) {
                if let Some(title) = &summary.title {
                    println!("Title: {title}");
                }
                if let Some(life) = summary.lifecycle {
                    println!("Creative lifecycle: {life}");
                }
                if let Some(bpm) = summary.bpm {
                    println!("BPM: {bpm}");
                }
            }
            println!();
            match &status.current_version {
                Some(v) => println!("Current Version: {} — {}", v.display, v.name),
                None => println!("Current Version: none"),
            }
            match &status.current_checkpoint {
                Some(c) => println!("Latest checkpoint: {}", &c.to_string()[..8]),
                None => println!("Latest checkpoint: none"),
            }
            println!();
            println!("Working project:");
            println!("  tracked files: {}", status.tracked_files);
            if status.checkpoint_needed {
                println!(
                    "  changes since checkpoint: {} modified/new, {} deleted",
                    status.modified_or_new, status.deleted_since_checkpoint
                );
                println!("  checkpoint needed: yes");
            } else {
                println!("  checkpoint needed: no");
            }
            println!();
            println!("Storage:");
            println!(
                "  Local objects: {}",
                if status.local_store_ok {
                    "OK"
                } else {
                    "unavailable"
                }
            );
            println!("  Missing objects: {}", status.missing_objects);
            println!("  Corrupt objects: {}", status.corrupt_objects);
            if status.remote_configured {
                println!(
                    "  Remote objects: {}",
                    if status.remote_verified_current {
                        "verified for current snapshot"
                    } else {
                        "configured (current snapshot not verified remotely)"
                    }
                );
            } else {
                println!("  Remote objects: not configured");
            }
            println!();
            println!("Git:");
            println!(
                "  Local history: {}",
                if status.git.initialized {
                    "OK"
                } else {
                    "missing"
                }
            );
            if status.git.remotes.is_empty() {
                println!("  Remote: not configured");
            } else {
                for remote in &status.git.remotes {
                    println!("  Remote: {} ({})", remote.name, remote.url);
                }
            }
            println!();
            println!(
                "Publication: {}",
                if status.git_published_current && status.remote_verified_current {
                    "published (remote objects verified + Git pushed)"
                } else if status.remote_verified_current {
                    "remote objects verified; Git not pushed"
                } else if status.remote_configured {
                    "unpublished"
                } else {
                    "local only (remote backup not configured)"
                }
            );
        }
        Commands::Verify { project } => {
            let report = verify_working_tree(&project)?;
            if report.ok() {
                println!("Working tree integrity");
                println!("{} objects found", report.verified + report.failed);
                println!("{} verified", report.verified);
                println!("0 unresolved");
                println!();
                println!("Snapshot ready.");
            } else {
                println!("Snapshot blocked");
                println!();
                println!("{} required object(s) are unresolved:", report.failed);
                for f in report
                    .findings
                    .iter()
                    .filter(|f| f.kind != versione_core::VerifyIssueKind::Verified)
                {
                    println!("  {}: {}", f.path, f.detail.as_deref().unwrap_or("failed"));
                }
                return Err("verification failed".into());
            }
        }
        Commands::Checkpoint { project } => print_checkpoint(create_checkpoint(&project)?)?,
        Commands::Snapshot { project } => print_checkpoint(snapshot_project(&project)?)?,
        Commands::Version { name, project } => {
            let report = keep_version(&project, &name)?;
            println!("Kept Version {} — {}", report.display, report.name);
            if report.created_new_checkpoint {
                println!("Snapshot: {}", &report.snapshot_id.to_string()[..8]);
            } else {
                println!(
                    "Promoted existing checkpoint {}",
                    &report.snapshot_id.to_string()[..8]
                );
            }
        }
        Commands::History {
            project,
            include_checkpoint,
            verbose,
        } => {
            let entries = list_history(&project, include_checkpoint)?;
            if entries.is_empty() {
                println!("No Versions yet.");
                return Ok(());
            }
            for entry in entries {
                match entry.kind {
                    HistoryEntryKind::Version => {
                        let display = entry.display.unwrap_or_else(|| "V??".into());
                        let name = entry.name.unwrap_or_default();
                        let marker = if entry.is_current { " *" } else { "" };
                        if verbose {
                            println!("{display}  {name}{marker}  ({})", entry.snapshot_id);
                        } else {
                            println!("{display}  {name}{marker}");
                        }
                    }
                    HistoryEntryKind::Checkpoint => {
                        let marker = if entry.is_current { " *" } else { "" };
                        if verbose {
                            println!("(checkpoint)  {}{marker}", entry.snapshot_id);
                        } else {
                            println!(
                                "(checkpoint)  {}{marker}",
                                &entry.snapshot_id.to_string()[..8]
                            );
                        }
                    }
                }
            }
        }
        Commands::Restore {
            version,
            to,
            project,
        } => {
            let report = restore_version_as_copy(&project, &version, &to)?;
            println!(
                "Restored {} — {}",
                report.version_display.unwrap_or_else(|| "Version".into()),
                report.version_name.unwrap_or_default()
            );
            println!("Files: {}", report.file_count);
            println!("Destination: {}", report.destination.display());
            println!("Original project was not modified.");
        }
        Commands::Publish { project, version } => {
            println!("Publishing: uploading objects, then pushing Git metadata…");
            let report = publish_project(
                &project,
                PublishOptions {
                    selector: version,
                    remote_override: None,
                    skip_git_push: false,
                },
            )?;
            if report.already_published {
                println!("Already published; remote objects reused.");
            }
            println!("Snapshot: {}", &report.snapshot_id.to_string()[..8]);
            println!("Objects uploaded: {}", report.objects_uploaded);
            println!("Objects reused: {}", report.objects_reused);
            println!(
                "Remote verified: {}",
                if report.remote_verified { "yes" } else { "no" }
            );
            println!(
                "Git pushed: {}",
                if report.git_pushed { "yes" } else { "no" }
            );
        }
        Commands::Storage { command } => match command {
            StorageCommands::SetRemote {
                project,
                bucket,
                region,
                endpoint,
                prefix,
                location_id,
            } => {
                let remote = RemoteStorageConfig {
                    kind: "s3".into(),
                    location_id,
                    bucket,
                    region,
                    endpoint,
                    prefix,
                };
                let _ = configure_remote_storage(&project, remote)?;
                println!("Remote object storage configured.");
                println!("Credentials are read from environment variables only:");
                println!("  VERSIONE_S3_ACCESS_KEY_ID / AWS_ACCESS_KEY_ID");
                println!("  VERSIONE_S3_SECRET_ACCESS_KEY / AWS_SECRET_ACCESS_KEY");
                println!("Secrets are never written to .versione/ or Git.");
            }
        },
        Commands::Project { command } => match command {
            ProjectCommands::Show { project } => {
                let summary = project_summary(&project)?;
                println!("Project id: {}", summary.project_id);
                println!("Root: {}", summary.root.display());
                println!(
                    "Title: {}",
                    summary.title.as_deref().unwrap_or("(untitled)")
                );
                if !summary.artist_aliases.is_empty() {
                    println!("Artists: {}", summary.artist_aliases.join(", "));
                }
                if let Some(life) = summary.lifecycle {
                    println!("Lifecycle: {life}");
                }
                if let Some(stage) = summary.release_stage {
                    println!("Release stage: {stage}");
                }
                if let Some(genre) = &summary.genre {
                    println!("Genre: {genre}");
                }
                if let Some(bpm) = summary.bpm {
                    print!("BPM: {bpm}");
                    if let (Some(root), Some(scale)) = (&summary.root_note, &summary.scale) {
                        print!("  {root} {scale}");
                    }
                    println!();
                }
                if let Some(channels) = summary.channel_count {
                    println!("Channels: {channels}");
                }
                if !summary.instrument_names.is_empty() {
                    println!("Instruments: {}", summary.instrument_names.join(", "));
                }
                println!("Named versions: {}", summary.version_count);
                println!("Unfinished tasks: {}", summary.unfinished_tasks);
                println!(
                    "Audio preview: {}",
                    if summary.has_audio_preview {
                        "yes"
                    } else {
                        "no"
                    }
                );
                println!(
                    "Arrangement image: {}",
                    if summary.has_arrangement_image {
                        "yes"
                    } else {
                        "no"
                    }
                );
                println!("Updated (UTC): {}", summary.updated_utc.to_rfc3339());
            }
            ProjectCommands::Edit {
                project,
                title,
                artist,
                genre,
                subgenre,
                bpm,
                root,
                scale,
                key,
                tag,
                instrument,
                description,
            } => {
                let profile = update_profile(&project, |p| {
                    if let Some(title) = title.clone() {
                        p.title = Some(title);
                    }
                    if let Some(artist) = artist.clone() {
                        if !p.artist_aliases.iter().any(|a| a == &artist) {
                            p.artist_aliases.push(artist);
                        }
                    }
                    if let Some(genre) = genre.clone() {
                        p.musical.genre = Some(genre);
                    }
                    if let Some(subgenre) = subgenre.clone() {
                        p.musical.subgenre = Some(subgenre);
                    }
                    if let Some(bpm) = bpm {
                        p.musical.bpm = Some(bpm);
                    }
                    if let Some(root) = root.clone() {
                        p.musical.root_note = Some(root);
                    }
                    if let Some(scale) = scale.clone() {
                        p.musical.scale = Some(scale);
                    }
                    if let Some(key) = key.clone() {
                        p.musical.key = Some(key);
                    }
                    for t in &tag {
                        if !p.tags.iter().any(|x| x == t) {
                            p.tags.push(t.clone());
                        }
                    }
                    for name in &instrument {
                        if !p.instruments.iter().any(|i| &i.name == name) {
                            p.instruments.push(versione_core::InstrumentUsage {
                                name: name.clone(),
                                manufacturer: None,
                                kind: None,
                                role: None,
                                notes: None,
                            });
                        }
                    }
                    if let Some(description) = description.clone() {
                        p.description = Some(description);
                    }
                })?;
                let _ = register_project(&project);
                println!("Updated profile for {}", profile.project_id);
            }
        },
        Commands::Lifecycle { command } => match command {
            LifecycleCommands::Set { state, project } => {
                let parsed = LifecycleState::parse(&state).ok_or_else(|| {
                    format!(
                        "unknown lifecycle '{state}'; expected draft|creative-pool|active|publishing|released|archived"
                    )
                })?;
                let profile = set_lifecycle(&project, parsed)?;
                let _ = register_project(&project);
                println!(
                    "Lifecycle: {}",
                    profile.lifecycle.unwrap_or(LifecycleState::Draft)
                );
            }
        },
        Commands::Release { command } => match command {
            ReleaseCommands::Status { project } => {
                let profile = load_or_default_profile(&ProjectPaths::from_root(&project))?;
                let summary = project_summary(&project)?;
                println!(
                    "Lifecycle: {}",
                    summary
                        .lifecycle
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| "unset".into())
                );
                println!(
                    "Release stage: {}",
                    summary
                        .release_stage
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| "unset".into())
                );
                if let Some(title) = profile.release.release_title {
                    println!("Release title: {title}");
                }
                if let Some(label) = profile.release.label {
                    println!("Label: {label}");
                }
            }
            ReleaseCommands::SetStage { stage, project } => {
                let parsed = ReleaseStage::parse(&stage).ok_or_else(|| {
                    format!(
                        "unknown release stage '{stage}'; expected candidate|selected|mixing|mastering|artwork|metadata|submitted|scheduled|released"
                    )
                })?;
                let profile = set_release_stage(&project, parsed)?;
                let _ = register_project(&project);
                println!(
                    "Release stage: {}",
                    profile
                        .release
                        .stage
                        .map(|s| s.to_string())
                        .unwrap_or_default()
                );
            }
        },
        Commands::Note { command } => match command {
            NoteCommands::Add {
                project,
                text,
                category,
            } => {
                let note = add_note(&project, text, category)?;
                println!("Added note {}", note.id);
            }
            NoteCommands::List { project } => {
                for note in list_notes(&project)? {
                    let cat = note.category.unwrap_or_else(|| "note".into());
                    println!("{}  [{cat}]  {}", &note.id.to_string()[..8], note.body);
                }
            }
            NoteCommands::Show { project, id } => {
                let note = show_note(&project, &id)?;
                println!("id: {}", note.id);
                if let Some(cat) = note.category {
                    println!("category: {cat}");
                }
                println!("{}", note.body);
            }
        },
        Commands::Task { command } => match command {
            TaskCommands::Add {
                project,
                text,
                priority,
                category,
            } => {
                let priority = priority
                    .as_deref()
                    .and_then(TaskPriority::parse)
                    .unwrap_or_default();
                let task = add_task(
                    &project,
                    NewTask {
                        text,
                        priority,
                        category,
                        ..Default::default()
                    },
                )?;
                println!("Added task {}  [{}]", task.id, task.status.as_str());
            }
            TaskCommands::List { project } => {
                for task in list_tasks(&project)? {
                    println!(
                        "{}  [{:7}]  {}",
                        &task.id.to_string()[..8],
                        task.status.as_str(),
                        task.text
                    );
                }
            }
            TaskCommands::Done { project, id } => {
                let task = complete_task(&project, &id)?;
                println!("Done: {} — {}", &task.id.to_string()[..8], task.text);
            }
        },
        Commands::Preview { command } => match command {
            PreviewCommands::Audio { command } => match command {
                PreviewAddCommands::Add {
                    file,
                    project,
                    label,
                } => {
                    let media = add_media_file(&project, &file, MediaKind::AudioPreview, label)?;
                    println!(
                        "Audio preview stored as object {}",
                        &media.object_id.to_string()[..12]
                    );
                }
            },
            PreviewCommands::Image { command } => match command {
                PreviewAddCommands::Add {
                    file,
                    project,
                    label,
                } => {
                    let media =
                        add_media_file(&project, &file, MediaKind::ArrangementImage, label)?;
                    println!(
                        "Arrangement image stored as object {}",
                        &media.object_id.to_string()[..12]
                    );
                }
            },
        },
        Commands::Screenshot { command } => match command {
            ScreenshotCommands::Add {
                file,
                project,
                label,
                snapshot,
            } => {
                let snapshot_id = match snapshot {
                    Some(sel) => {
                        if let Some(id) = versione_core::SnapshotId::parse(&sel) {
                            Some(id)
                        } else {
                            Some(
                                versione_core::resolve_version_selector(&project, &sel)?
                                    .snapshot_id,
                            )
                        }
                    }
                    None => None,
                };
                let media = add_screenshot(
                    &project,
                    &file,
                    &AddScreenshotOptions {
                        label,
                        snapshot_id,
                        project_kind: None,
                    },
                )?;
                println!("Screenshot attached (optional visual memory)");
                println!("media_id: {}", media.id);
                println!("object: {}", &media.object_id.to_string()[..12]);
                if let Some(sid) = &media.related_snapshot_id {
                    println!("snapshot: {}", &sid.to_string()[..8]);
                } else {
                    println!("snapshot: (none — attach after first checkpoint to link)");
                }
                println!("Note: screenshots do not affect musical snapshot verification.");
            }
            ScreenshotCommands::List { project } => {
                let items = list_screenshots(&project)?;
                if items.is_empty() {
                    println!("No screenshots attached.");
                } else {
                    for m in items {
                        let snap = m
                            .related_snapshot_id
                            .as_ref()
                            .map(|s| s.to_string()[..8].to_string())
                            .unwrap_or_else(|| "-".into());
                        let label = m.label.as_deref().unwrap_or("");
                        println!(
                            "{}  snap:{}  obj:{}  {}",
                            &m.id.to_string()[..8],
                            snap,
                            &m.object_id.to_string()[..8],
                            label
                        );
                    }
                }
            }
        },
        Commands::Library { command } => match command {
            LibraryCommands::Add { project } => {
                let entry = register_project(&project)?;
                println!("Registered {}", entry.project_id);
                println!("Root: {}", entry.root.display());
            }
            LibraryCommands::Rebuild => {
                let summaries = rebuild_library()?;
                println!("Rebuilt library index: {} projects", summaries.len());
            }
        },
        Commands::Find {
            genre,
            bpm,
            scale,
            key,
            artist,
            instrument,
            tag,
            state,
            name,
            inactive_for,
            unfinished,
        } => {
            let (bpm_min, bpm_max) = match bpm {
                Some(spec) => parse_bpm_range(&spec)?,
                None => (None, None),
            };
            let lifecycle = match state {
                Some(s) => {
                    Some(LifecycleState::parse(&s).ok_or_else(|| format!("unknown state '{s}'"))?)
                }
                None => None,
            };
            let inactive_for_days = match inactive_for {
                Some(s) => Some(parse_inactive_days(&s)?),
                None => None,
            };
            let query = FindQuery {
                genre,
                bpm_min,
                bpm_max,
                scale,
                key,
                artist,
                instrument,
                tag,
                lifecycle,
                name_contains: name,
                inactive_for_days,
                has_unfinished_tasks: if unfinished { Some(true) } else { None },
                ..Default::default()
            };
            let hits = find_projects(&query)?;
            if hits.is_empty() {
                println!("No matching projects.");
                return Ok(());
            }
            for hit in hits {
                let title = hit.title.unwrap_or_else(|| {
                    hit.root
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| hit.root.display().to_string())
                });
                let life = hit
                    .lifecycle
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "unset".into());
                let bpm = hit
                    .bpm
                    .map(|b| format!("{b}"))
                    .unwrap_or_else(|| "-".into());
                println!("{title}  [{life}]  bpm={bpm}  {}", hit.root.display());
            }
        }
        Commands::Rediscover => {
            let hints = rediscovery_hints()?;
            if hints.is_empty() {
                println!("No rediscovery hints from the current library index.");
                return Ok(());
            }
            for hint in hints {
                println!("{}", hint.message);
            }
        }
        Commands::Stats { command } => {
            let overview = compute_overview(StatsWindow::AllTime)?;
            match command.unwrap_or(StatsCommands::Projects) {
                StatsCommands::Projects | StatsCommands::Workflow => {
                    println!("VERSIONE / INSIGHTS");
                    println!();
                    println!("Projects                 {}", overview.projects);
                    println!("Released                 {}", overview.released);
                    println!("Creative Pool            {}", overview.creative_pool);
                    println!("Active                   {}", overview.active);
                    println!("Unfinished tasks         {}", overview.unfinished_tasks);
                    if let Some(ratio) = overview.release_completion_ratio {
                        println!("Release completion       {:.1}%", ratio * 100.0);
                    }
                    if let Some(avg) = overview.average_versions {
                        println!("Average named versions   {avg:.1}");
                    }
                    for row in &overview.by_lifecycle {
                        println!("  {:<16} {}", row.label, row.count);
                    }
                }
                StatsCommands::Music => {
                    let music = compute_music_stats(StatsWindow::AllTime)?;
                    println!("VERSIONE / MUSIC");
                    println!();
                    if let Some(bpm) = overview.median_bpm {
                        println!("Median BPM               {bpm}");
                    } else {
                        println!("Median BPM               (no data)");
                    }
                    println!(
                        "Most used root           {}",
                        overview.most_used_root.as_deref().unwrap_or("(no data)")
                    );
                    println!(
                        "Most used scale          {}",
                        overview.most_used_scale.as_deref().unwrap_or("(no data)")
                    );
                    println!(
                        "Most used genre          {}",
                        overview.most_used_genre.as_deref().unwrap_or("(no data)")
                    );
                    if !music.bpm_distribution.is_empty() {
                        println!();
                        println!("BPM distribution:");
                        for (bucket, count) in music.bpm_distribution {
                            println!("  {bucket}: {count}");
                        }
                    }
                }
                StatsCommands::Instruments => {
                    println!("VERSIONE / INSTRUMENTS");
                    println!();
                    println!(
                        "Most used instrument     {}",
                        overview
                            .most_used_instrument
                            .as_deref()
                            .unwrap_or("(no data)")
                    );
                    if let Some(avg) = overview.average_channels {
                        println!("Average channels         {avg:.1}");
                    } else {
                        println!("Average channels         (no data)");
                    }
                }
            }
        }
        Commands::HashFile { path } => {
            let id = hash_file(&path)?;
            println!("{id}");
        }
    }
    Ok(())
}
