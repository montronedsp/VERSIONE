//! Prepare a self-contained REAPER representation for VERSIONE snapshots.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::collect::{collect_into_media, CollectionOptions, CollectionReport};
use crate::daw::reaper::parser::{inspect_rpp, ReaperReport, ReferenceClass};
use crate::daw::reaper::rewrite::{assert_rewrites_applied, rewrite_rpp_references};
use crate::error::{Error, ErrorKind, Result};
use crate::project::{read_toml, write_toml_atomic, ProjectPaths};

pub const CONTENT_OVERRIDES_FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentOverrideEntry {
    /// Project-relative path as it appears in the snapshot manifest.
    pub project_relative: String,
    /// Path relative to `.versione/` for staged snapshot bytes.
    pub staged_relative: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ContentOverrideDocument {
    pub format_version: u32,
    #[serde(default)]
    pub overrides: Vec<ContentOverrideEntry>,
}

#[derive(Debug, Clone)]
pub struct ReaperPrepareReport {
    pub inspection: ReaperReport,
    pub collection: CollectionReport,
    pub replacements: Vec<(String, String)>,
    pub staged_rpp: PathBuf,
    pub project_relative_rpp: String,
    pub original_rpp_unchanged: bool,
}

impl ProjectPaths {
    pub fn content_overrides_path(&self) -> PathBuf {
        self.versione_dir.join("content_overrides.toml")
    }

    pub fn staging_dir(&self) -> PathBuf {
        self.versione_dir.join("staging")
    }
}

/// Discover → collect externals → stage rewritten RPP (working RPP untouched).
pub fn prepare_reaper_self_contained(
    root: &Path,
    rpp_path: &Path,
    extra_external: &[PathBuf],
) -> Result<ReaperPrepareReport> {
    let paths = ProjectPaths::from_root(root);
    if !paths.config_path().exists() {
        return Err(
            Error::new(ErrorKind::InvalidProject, "VERSIONE is not initialized").with_path(root),
        );
    }

    let original_bytes = fs::read(rpp_path)
        .map_err(|e| Error::io("failed to read rpp before prepare", rpp_path, e))?;
    let inspection = inspect_rpp(rpp_path)?;

    // Block early on unsafe / unsupported required refs we cannot collect.
    for r in inspection.required_blocking() {
        if matches!(
            r.class,
            ReferenceClass::Unsafe | ReferenceClass::Unsupported | ReferenceClass::Unresolved
        ) {
            return Err(Error::new(
                ErrorKind::Invariant,
                format!(
                    "Snapshot blocked\n\nUnresolved REAPER media reference ({}) at {}:\n{}",
                    format!("{:?}", r.class).to_ascii_lowercase(),
                    r.location_hint,
                    r.source_path
                ),
            ));
        }
        if r.class == ReferenceClass::Missing {
            return Err(Error::new(
                ErrorKind::Invariant,
                format!(
                    "Snapshot blocked\n\nMissing REAPER media reference at {}:\n{}",
                    r.location_hint, r.source_path
                ),
            ));
        }
    }

    let mut externals: Vec<PathBuf> = inspection
        .references
        .iter()
        .filter(|r| r.class == ReferenceClass::External)
        .filter_map(|r| r.resolved_absolute.clone())
        .collect();
    // Dedup by path
    externals.sort();
    externals.dedup();
    for p in extra_external {
        if !externals.iter().any(|e| e == p) {
            externals.push(p.clone());
        }
    }

    let collection = collect_into_media(
        root,
        &CollectionOptions {
            external_paths: externals,
            media_dir: None,
        },
    )?;

    // Map original source_path → collected destination for each external ref.
    let mut replacements = Vec::new();
    for r in &inspection.references {
        if r.class != ReferenceClass::External {
            continue;
        }
        let Some(abs) = &r.resolved_absolute else {
            continue;
        };
        let dest_rel = find_collected_destination(root, &collection, abs)?;
        replacements.push((r.source_path.clone(), dest_rel));
    }

    // Also map from collection document for extras
    let text = fs::read_to_string(rpp_path)
        .map_err(|e| Error::io("failed to read rpp for rewrite", rpp_path, e))?;
    let rewritten = rewrite_rpp_references(&text, &replacements)?;
    assert_rewrites_applied(&text, &rewritten, &inspection.references, &replacements)?;

    // Verify rewritten FILE refs resolve inside project.
    verify_staged_text_refs(root, &rewritten)?;

    let project_relative_rpp = path_relative(root, rpp_path)?;
    let staging = paths.staging_dir();
    fs::create_dir_all(&staging)
        .map_err(|e| Error::io("failed to create staging dir", &staging, e))?;
    let staged_name = rpp_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "project.rpp".into());
    let staged_rpp = staging.join(&staged_name);
    let tmp = staged_rpp.with_extension("rpp.versione-partial");
    fs::write(&tmp, rewritten.as_bytes())
        .map_err(|e| Error::io("failed to write staged rpp", &tmp, e))?;
    fs::rename(&tmp, &staged_rpp).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        Error::io("failed to finalize staged rpp", &staged_rpp, e)
    })?;

    let doc = ContentOverrideDocument {
        format_version: CONTENT_OVERRIDES_FORMAT_VERSION,
        overrides: vec![ContentOverrideEntry {
            project_relative: project_relative_rpp.clone(),
            staged_relative: format!("staging/{staged_name}"),
        }],
    };
    write_toml_atomic(&paths.content_overrides_path(), &doc)?;

    let after = fs::read(rpp_path)
        .map_err(|e| Error::io("failed to re-read rpp after prepare", rpp_path, e))?;
    let original_rpp_unchanged = after == original_bytes;

    Ok(ReaperPrepareReport {
        inspection,
        collection,
        replacements,
        staged_rpp,
        project_relative_rpp,
        original_rpp_unchanged,
    })
}

fn find_collected_destination(
    root: &Path,
    report: &CollectionReport,
    abs: &Path,
) -> Result<String> {
    // Prefer exact object match via re-hash of source.
    let oid = crate::objects::hash_file(abs)?;
    if let Some(item) = report.items.iter().find(|i| i.object_id == oid.as_str()) {
        return Ok(item.destination_relative.clone());
    }
    // Load durable collection doc
    let paths = ProjectPaths::from_root(root);
    if let Some(doc) = crate::collect::load_collection_doc(&paths)? {
        if let Some(item) = doc.items.iter().find(|i| i.object_id == oid.as_str()) {
            return Ok(item.destination_relative.clone());
        }
    }
    Err(Error::new(
        ErrorKind::Invariant,
        format!(
            "collected destination not found for external media {}",
            abs.display()
        ),
    ))
}

fn verify_staged_text_refs(root: &Path, rewritten: &str) -> Result<()> {
    let project_dir = root;
    // Write to temp parse via parse_rpp_text with a virtual path
    let fake = root.join("__versione_staged__.rpp");
    let report = crate::daw::reaper::parser::parse_rpp_text(rewritten, &fake, project_dir)?;
    for r in &report.references {
        match r.class {
            ReferenceClass::ProjectLocal => {}
            ReferenceClass::External
            | ReferenceClass::Missing
            | ReferenceClass::Unsafe
            | ReferenceClass::Unresolved
            | ReferenceClass::Unsupported => {
                return Err(Error::new(
                    ErrorKind::Invariant,
                    format!(
                        "staged self-contained RPP still has non-local reference at {}: {}",
                        r.location_hint, r.source_path
                    ),
                ));
            }
        }
    }
    Ok(())
}

fn path_relative(root: &Path, abs: &Path) -> Result<String> {
    let rel = abs.strip_prefix(root).map_err(|_| {
        Error::new(ErrorKind::PathEscape, "rpp path escaped project root").with_path(abs)
    })?;
    Ok(rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/"))
}

pub fn load_content_overrides(paths: &ProjectPaths) -> Result<Option<ContentOverrideDocument>> {
    if !paths.content_overrides_path().exists() {
        return Ok(None);
    }
    let doc: ContentOverrideDocument = read_toml(&paths.content_overrides_path())?;
    if doc.format_version != CONTENT_OVERRIDES_FORMAT_VERSION {
        return Err(Error::new(
            ErrorKind::UnsupportedFormat,
            format!(
                "unsupported content_overrides format_version {}",
                doc.format_version
            ),
        ));
    }
    Ok(Some(doc))
}

fn join_project_relative(root: &Path, project_relative: &str) -> PathBuf {
    let mut p = root.to_path_buf();
    for part in project_relative.split(['/', '\\']) {
        if part.is_empty() || part == "." {
            continue;
        }
        p.push(part);
    }
    p
}

/// Resolve on-disk bytes used for a scanned project-relative path (staging override).
pub fn resolve_content_path(paths: &ProjectPaths, project_relative: &str) -> Result<PathBuf> {
    if let Some(doc) = load_content_overrides(paths)? {
        if let Some(entry) = doc
            .overrides
            .iter()
            .find(|e| e.project_relative == project_relative)
        {
            let staged = {
                let mut p = paths.versione_dir.clone();
                for part in entry.staged_relative.split(['/', '\\']) {
                    if !part.is_empty() {
                        p.push(part);
                    }
                }
                p
            };
            if !staged.is_file() {
                return Err(
                    Error::new(ErrorKind::NotFound, "staged content override missing")
                        .with_path(staged),
                );
            }
            let canon_v = fs::canonicalize(&paths.versione_dir)
                .unwrap_or_else(|_| paths.versione_dir.clone());
            let canon_s = fs::canonicalize(&staged).unwrap_or(staged.clone());
            if !canon_s.starts_with(&canon_v) {
                return Err(Error::new(
                    ErrorKind::PathEscape,
                    "content override escaped .versione",
                )
                .with_path(staged));
            }
            return Ok(staged);
        }
    }
    Ok(join_project_relative(&paths.root, project_relative))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{init_project, InitOptions};
    use tempfile::tempdir;

    #[test]
    fn prepares_self_contained_without_touching_original() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("proj");
        fs::create_dir_all(&root).unwrap();
        let outside = dir.path().join("ext");
        fs::create_dir_all(&outside).unwrap();
        let wav = outside.join("kick.wav");
        fs::write(&wav, b"RIFFDATA").unwrap();
        let rpp = root.join("Song.rpp");
        let body = format!(
            "<REAPER_PROJECT 0.1 \"6.0\" 1\n  <TRACK\n    <ITEM\n      <SOURCE WAVE\n        FILE \"{}\"\n      >\n    >\n  >\n>\n",
            wav.display()
        );
        fs::write(&rpp, &body).unwrap();
        init_project(&root, &InitOptions::default()).unwrap();
        let report = prepare_reaper_self_contained(&root, &rpp, &[]).unwrap();
        assert!(report.original_rpp_unchanged);
        assert_eq!(fs::read_to_string(&rpp).unwrap(), body);
        let staged = fs::read_to_string(&report.staged_rpp).unwrap();
        assert!(staged.contains("Media/imported/"));
        assert!(!staged.contains(&wav.display().to_string()) || staged.contains("Media/imported"));
        assert!(root.join("Media/imported/kick.wav").is_file());
        assert!(wav.is_file());
    }
}
