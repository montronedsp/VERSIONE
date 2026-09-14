//! Conservative line-oriented RPP scanner for FILE-backed media references.

use std::fs;
use std::path::{Path, PathBuf};

use crate::daw::DawReferenceSummary;
use crate::error::{Error, ErrorKind, Result};

/// How a discovered path relates to the project / filesystem.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReferenceClass {
    ProjectLocal,
    External,
    Missing,
    Unsupported,
    Unresolved,
    Unsafe,
}

/// Kind of file-backed dependency when identifiable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReferenceKind {
    Audio,
    Midi,
    Video,
    Subproject,
    ImpulseResponse,
    OtherFile,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReaperReference {
    /// Exact path string as written in the RPP (unescaped).
    pub source_path: String,
    pub reference_kind: ReferenceKind,
    pub class: ReferenceClass,
    /// Byte offset of the FILE path token in the original text (for rewrite).
    pub path_byte_start: usize,
    pub path_byte_end: usize,
    /// Absolute resolved path when resolution succeeded.
    pub resolved_absolute: Option<PathBuf>,
    /// Project-relative path when the file is inside the project tree.
    pub project_relative: Option<String>,
    pub required: bool,
    pub location_hint: String,
}

#[derive(Debug, Clone)]
pub struct ReaperReport {
    pub rpp_path: PathBuf,
    pub project_dir: PathBuf,
    pub references: Vec<ReaperReference>,
    pub notes: Vec<String>,
}

impl ReaperReport {
    pub fn summary(&self) -> DawReferenceSummary {
        let mut project_local = 0usize;
        let mut external = 0usize;
        let mut missing = 0usize;
        let mut unsupported = 0usize;
        let mut unresolved = 0usize;
        let mut unsafe_path = 0usize;
        for r in &self.references {
            match r.class {
                ReferenceClass::ProjectLocal => project_local += 1,
                ReferenceClass::External => external += 1,
                ReferenceClass::Missing => missing += 1,
                ReferenceClass::Unsupported => unsupported += 1,
                ReferenceClass::Unresolved => unresolved += 1,
                ReferenceClass::Unsafe => unsafe_path += 1,
            }
        }
        DawReferenceSummary {
            total: self.references.len(),
            project_local,
            external,
            collected: 0,
            missing,
            unsupported,
            unresolved,
            unsafe_path,
        }
    }

    pub fn required_blocking(&self) -> Vec<&ReaperReference> {
        self.references
            .iter()
            .filter(|r| {
                r.required
                    && matches!(
                        r.class,
                        ReferenceClass::Missing
                            | ReferenceClass::External
                            | ReferenceClass::Unresolved
                            | ReferenceClass::Unsafe
                            | ReferenceClass::Unsupported
                    )
            })
            .collect()
    }
}

/// Inspect a REAPER project file (read-only).
pub fn inspect_rpp(rpp_path: &Path) -> Result<ReaperReport> {
    let text = fs::read_to_string(rpp_path)
        .map_err(|e| Error::io("failed to read REAPER project", rpp_path, e))?;
    if text.contains('\0') {
        return Err(Error::new(
            ErrorKind::UnsupportedFormat,
            "REAPER project appears binary or contains NUL bytes",
        )
        .with_path(rpp_path));
    }
    let project_dir = rpp_path.parent().ok_or_else(|| {
        Error::new(ErrorKind::InvalidProject, "rpp has no parent directory").with_path(rpp_path)
    })?;
    let mut report = parse_rpp_text(&text, rpp_path, project_dir)?;
    report
        .notes
        .push("Inspection is read-only; the canonical .rpp is never modified by VERSIONE.".into());
    Ok(report)
}

/// Parse RPP text and classify FILE references relative to `project_dir`.
pub fn parse_rpp_text(text: &str, rpp_path: &Path, project_dir: &Path) -> Result<ReaperReport> {
    if !text.ltrim_start().starts_with('<') {
        // Tolerant: some exports may have a BOM or preamble.
        if !text.contains("<REAPER_PROJECT") && !text.contains("<PROJECT") {
            return Err(Error::new(
                ErrorKind::UnsupportedFormat,
                "text does not look like a REAPER project (missing REAPER_PROJECT chunk)",
            )
            .with_path(rpp_path));
        }
    }

    let file_hits = find_file_directives(text)?;
    let mut references = Vec::new();
    let mut notes = Vec::new();

    for hit in file_hits {
        let classified = classify_reference(project_dir, &hit, text)?;
        references.push(classified);
    }

    // Deduplicate logical paths for notes, but keep all occurrences for rewrite.
    let unique_external = references
        .iter()
        .filter(|r| r.class == ReferenceClass::External)
        .map(|r| r.source_path.clone())
        .collect::<std::collections::BTreeSet<_>>();
    if !unique_external.is_empty() {
        notes.push(format!(
            "{} unique external media path(s) discovered",
            unique_external.len()
        ));
    }

    Ok(ReaperReport {
        rpp_path: rpp_path.to_path_buf(),
        project_dir: project_dir.to_path_buf(),
        references,
        notes,
    })
}

/// Discover references (alias of inspect without notes about mutation).
pub fn discover_references(rpp_path: &Path) -> Result<Vec<ReaperReference>> {
    Ok(inspect_rpp(rpp_path)?.references)
}

#[derive(Debug)]
struct FileHit {
    raw_path: String,
    path_byte_start: usize,
    path_byte_end: usize,
    line_no: usize,
    source_type_hint: Option<String>,
}

fn find_file_directives(text: &str) -> Result<Vec<FileHit>> {
    let mut hits = Vec::new();
    let mut source_stack: Vec<String> = Vec::new();
    let mut byte_pos = 0usize;
    let mut line_no = 0usize;

    for line in text.split_inclusive('\n') {
        line_no += 1;
        let trimmed = line.trim_start();
        let indent_ws = line.len() - trimmed.len();

        // Track SOURCE chunk open/close for kind hints (best-effort).
        let t = trimmed.trim_end_matches(['\r', '\n']);
        if let Some(rest) = t.strip_prefix("<SOURCE") {
            let ty = rest.split_whitespace().next().unwrap_or("").to_string();
            source_stack.push(ty);
        } else if t == ">" || t.starts_with('>') {
            let _ = source_stack.pop();
        }

        if let Some(hit) =
            parse_file_line(line, byte_pos + indent_ws, line_no, source_stack.last())?
        {
            hits.push(hit);
        }
        byte_pos += line.len();
    }
    Ok(hits)
}

fn parse_file_line(
    line: &str,
    line_byte_start: usize,
    line_no: usize,
    source_ty: Option<&String>,
) -> Result<Option<FileHit>> {
    let trimmed = line.trim_start();
    if strip_file_token(trimmed).is_none() {
        return Ok(None);
    }
    let token_in_line = line.len() - trimmed.len();
    let file_kw = trimmed
        .find("FILE")
        .or_else(|| trimmed.find("file"))
        .unwrap_or(0);
    let after_kw = &trimmed[file_kw + 4..];
    let after_kw = after_kw.strip_prefix(':').unwrap_or(after_kw);
    let trimmed_after = after_kw.trim_start();
    let abs_after =
        line_byte_start + token_in_line + file_kw + 4 + (after_kw.len() - trimmed_after.len());

    let (raw_path, path_start, path_end) = parse_path_token(trimmed_after, abs_after)?;
    if raw_path.is_empty() {
        return Ok(None);
    }
    if looks_like_non_path(&raw_path) {
        return Ok(None);
    }

    Ok(Some(FileHit {
        raw_path,
        path_byte_start: path_start,
        path_byte_end: path_end,
        line_no,
        source_type_hint: source_ty.cloned(),
    }))
}

fn strip_file_token(trimmed: &str) -> Option<&str> {
    if trimmed.len() >= 4
        && trimmed.as_bytes()[..4].eq_ignore_ascii_case(b"FILE")
        && (trimmed.len() == 4
            || trimmed.as_bytes()[4].is_ascii_whitespace()
            || trimmed.as_bytes()[4] == b':')
    {
        Some(trimmed)
    } else {
        None
    }
}

fn parse_path_token(after_kw: &str, abs_start: usize) -> Result<(String, usize, usize)> {
    let s = after_kw;
    if let Some(rest) = s.strip_prefix('"') {
        let mut out = String::new();
        let mut it = rest.char_indices().peekable();
        while let Some((i, c)) = it.next() {
            match c {
                '"' => {
                    let path_byte_start = abs_start + 1;
                    // `out` is the decoded path; byte end uses original span up to closing quote.
                    let path_byte_end = abs_start + 1 + i;
                    return Ok((out, path_byte_start, path_byte_end));
                }
                '\\' => match it.next() {
                    Some((_, '"')) => out.push('"'),
                    Some((_, '\\')) => out.push('\\'),
                    Some((_, other)) => {
                        out.push('\\');
                        out.push(other);
                    }
                    None => out.push('\\'),
                },
                other => out.push(other),
            }
        }
        return Err(Error::new(
            ErrorKind::UnsupportedFormat,
            "malformed quoted FILE path in RPP (missing closing quote)",
        ));
    }

    let end = s.find(|c: char| c.is_whitespace()).unwrap_or(s.len());
    let path = s[..end].to_string();
    Ok((path.clone(), abs_start, abs_start + path.len()))
}

fn looks_like_non_path(s: &str) -> bool {
    if s.chars()
        .all(|c| c.is_ascii_digit() || c == '.' || c == '-')
        && !s.contains('/')
        && !s.contains('\\')
    {
        return true;
    }
    if s.starts_with('{') && s.ends_with('}') {
        return true;
    }
    false
}

fn classify_reference(project_dir: &Path, hit: &FileHit, _text: &str) -> Result<ReaperReference> {
    let kind = kind_from_hint_and_path(hit.source_type_hint.as_deref(), &hit.raw_path);
    let location_hint = format!("line {}", hit.line_no);

    if hit.raw_path.contains('\0') {
        return Ok(ReaperReference {
            source_path: hit.raw_path.clone(),
            reference_kind: kind,
            class: ReferenceClass::Unsafe,
            path_byte_start: hit.path_byte_start,
            path_byte_end: hit.path_byte_end,
            resolved_absolute: None,
            project_relative: None,
            required: true,
            location_hint,
        });
    }

    // Reject attempts to target VERSIONE internals via relative traversal in the reference text.
    if path_text_targets_versione(&hit.raw_path) {
        return Ok(ReaperReference {
            source_path: hit.raw_path.clone(),
            reference_kind: kind,
            class: ReferenceClass::Unsafe,
            path_byte_start: hit.path_byte_start,
            path_byte_end: hit.path_byte_end,
            resolved_absolute: None,
            project_relative: None,
            required: true,
            location_hint,
        });
    }

    let resolved = resolve_against_project(project_dir, &hit.raw_path);
    match resolved {
        PathResolution::Unsafe => Ok(ReaperReference {
            source_path: hit.raw_path.clone(),
            reference_kind: kind,
            class: ReferenceClass::Unsafe,
            path_byte_start: hit.path_byte_start,
            path_byte_end: hit.path_byte_end,
            resolved_absolute: None,
            project_relative: None,
            required: true,
            location_hint,
        }),
        PathResolution::Unresolved => Ok(ReaperReference {
            source_path: hit.raw_path.clone(),
            reference_kind: kind,
            class: ReferenceClass::Unresolved,
            path_byte_start: hit.path_byte_start,
            path_byte_end: hit.path_byte_end,
            resolved_absolute: None,
            project_relative: None,
            required: true,
            location_hint,
        }),
        PathResolution::Ok(abs) => {
            if !abs.exists() {
                return Ok(ReaperReference {
                    source_path: hit.raw_path.clone(),
                    reference_kind: kind,
                    class: ReferenceClass::Missing,
                    path_byte_start: hit.path_byte_start,
                    path_byte_end: hit.path_byte_end,
                    resolved_absolute: Some(abs),
                    project_relative: None,
                    required: true,
                    location_hint,
                });
            }
            if abs.is_symlink() {
                return Ok(ReaperReference {
                    source_path: hit.raw_path.clone(),
                    reference_kind: kind,
                    class: ReferenceClass::Unsafe,
                    path_byte_start: hit.path_byte_start,
                    path_byte_end: hit.path_byte_end,
                    resolved_absolute: Some(abs),
                    project_relative: None,
                    required: true,
                    location_hint,
                });
            }
            let canon_proj =
                fs::canonicalize(project_dir).unwrap_or_else(|_| project_dir.to_path_buf());
            let canon_proj = strip_extended_prefix(canon_proj);
            let canon_abs = fs::canonicalize(&abs).unwrap_or(abs.clone());
            let canon_abs = strip_extended_prefix(canon_abs);
            if canon_abs.starts_with(&canon_proj) {
                let rel = canon_abs.strip_prefix(&canon_proj).map(to_slash).ok();
                Ok(ReaperReference {
                    source_path: hit.raw_path.clone(),
                    reference_kind: kind,
                    class: ReferenceClass::ProjectLocal,
                    path_byte_start: hit.path_byte_start,
                    path_byte_end: hit.path_byte_end,
                    resolved_absolute: Some(canon_abs),
                    project_relative: rel,
                    required: true,
                    location_hint,
                })
            } else {
                Ok(ReaperReference {
                    source_path: hit.raw_path.clone(),
                    reference_kind: kind,
                    class: ReferenceClass::External,
                    path_byte_start: hit.path_byte_start,
                    path_byte_end: hit.path_byte_end,
                    resolved_absolute: Some(canon_abs),
                    project_relative: None,
                    required: true,
                    location_hint,
                })
            }
        }
    }
}

enum PathResolution {
    Ok(PathBuf),
    #[allow(dead_code)]
    Unresolved,
    Unsafe,
}

fn resolve_against_project(project_dir: &Path, raw: &str) -> PathResolution {
    let path = PathBuf::from(raw);
    if is_unc(raw) {
        return PathResolution::Ok(path);
    }
    if path.is_absolute() {
        return PathResolution::Ok(path);
    }
    // Relative to the .rpp directory (REAPER semantics), never process CWD.
    let joined = project_dir.join(&path);
    // Component-level parent-dir check before canonicalize.
    for c in Path::new(raw).components() {
        if matches!(c, std::path::Component::ParentDir) {
            // Allow .. only if final path stays inside project after normalize — still mark carefully.
            // We resolve then check containment later; but escaping via .. to outside becomes External or Unsafe.
            break;
        }
    }
    if raw.split(['/', '\\']).any(|p| p == ".versione") {
        return PathResolution::Unsafe;
    }
    PathResolution::Ok(joined)
}

fn is_unc(raw: &str) -> bool {
    raw.starts_with("\\\\") || raw.starts_with("//")
}

fn path_text_targets_versione(raw: &str) -> bool {
    raw.split(['/', '\\'])
        .any(|p| p.eq_ignore_ascii_case(".versione"))
}

fn kind_from_hint_and_path(hint: Option<&str>, path: &str) -> ReferenceKind {
    let hint = hint.unwrap_or("").to_ascii_uppercase();
    if hint.contains("MIDI") {
        return ReferenceKind::Midi;
    }
    if hint.contains("VIDEO") || hint.contains("MOV") {
        return ReferenceKind::Video;
    }
    if hint.contains("RPP_PROJECT") || hint == "RPP" {
        return ReferenceKind::Subproject;
    }
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "wav" | "wave" | "aif" | "aiff" | "flac" | "mp3" | "ogg" | "oga" | "opus" | "wv"
        | "caf" | "rex" | "rx2" | "bwf" => ReferenceKind::Audio,
        "mid" | "midi" => ReferenceKind::Midi,
        "mp4" | "mov" | "mkv" | "avi" | "webm" => ReferenceKind::Video,
        "rpp" => ReferenceKind::Subproject,
        "" => ReferenceKind::Unknown,
        _ => ReferenceKind::OtherFile,
    }
}

fn to_slash(path: &Path) -> String {
    path.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn strip_extended_prefix(path: PathBuf) -> PathBuf {
    let s = path.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\") {
        PathBuf::from(rest)
    } else {
        path
    }
}

trait LtrimStart {
    fn ltrim_start(&self) -> &str;
}
impl LtrimStart for str {
    fn ltrim_start(&self) -> &str {
        self.trim_start_matches('\u{feff}').trim_start()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn sample_rpp(file_line: &str) -> String {
        format!(
            "<REAPER_PROJECT 0.1 \"6.0\" 123\n  <TRACK\n    <ITEM\n      <SOURCE WAVE\n        {file_line}\n      >\n    >\n  >\n>\n"
        )
    }

    #[test]
    fn discovers_quoted_relative_media() {
        let dir = tempdir().unwrap();
        let media = dir.path().join("kick.wav");
        fs::write(&media, b"RIFF").unwrap();
        let rpp = dir.path().join("Song.rpp");
        fs::write(&rpp, sample_rpp("FILE \"kick.wav\"")).unwrap();
        let report = inspect_rpp(&rpp).unwrap();
        assert_eq!(report.references.len(), 1);
        assert_eq!(report.references[0].class, ReferenceClass::ProjectLocal);
    }

    #[test]
    fn discovers_external_absolute() {
        let dir = tempdir().unwrap();
        let outside = dir.path().join("outside");
        fs::create_dir_all(&outside).unwrap();
        let wav = outside.join("vox.wav");
        fs::write(&wav, b"RIFF").unwrap();
        let proj = dir.path().join("proj");
        fs::create_dir_all(&proj).unwrap();
        let rpp = proj.join("Song.rpp");
        let line = format!("FILE \"{}\"", wav.display());
        fs::write(&rpp, sample_rpp(&line)).unwrap();
        let report = inspect_rpp(&rpp).unwrap();
        assert_eq!(report.references[0].class, ReferenceClass::External);
    }

    #[test]
    fn missing_media_classified() {
        let dir = tempdir().unwrap();
        let rpp = dir.path().join("Song.rpp");
        fs::write(&rpp, sample_rpp("FILE \"gone.wav\"")).unwrap();
        let report = inspect_rpp(&rpp).unwrap();
        assert_eq!(report.references[0].class, ReferenceClass::Missing);
    }

    #[test]
    fn ignores_unrelated_quoted_strings() {
        let dir = tempdir().unwrap();
        let rpp = dir.path().join("Song.rpp");
        let text = "<REAPER_PROJECT 0.1 \"6.0\" 1\n  NAME \"My Track\"\n  <TRACK\n  >\n>\n";
        fs::write(&rpp, text).unwrap();
        let report = inspect_rpp(&rpp).unwrap();
        assert!(report.references.is_empty());
    }

    #[test]
    fn unicode_filename() {
        let dir = tempdir().unwrap();
        let name = "cafè-kick.wav";
        fs::write(dir.path().join(name), b"RIFF").unwrap();
        let rpp = dir.path().join("Song.rpp");
        fs::write(&rpp, sample_rpp(&format!("FILE \"{name}\""))).unwrap();
        let report = inspect_rpp(&rpp).unwrap();
        assert_eq!(report.references[0].class, ReferenceClass::ProjectLocal);
    }

    #[test]
    fn rejects_binary_nul() {
        let dir = tempdir().unwrap();
        let rpp = dir.path().join("Song.rpp");
        fs::write(&rpp, b"<REAPER_PROJECT\0>\n").unwrap();
        let err = inspect_rpp(&rpp).unwrap_err();
        assert_eq!(err.kind, ErrorKind::UnsupportedFormat);
    }
}
