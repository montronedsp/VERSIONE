//! Resolve a music project root for Enable VERSIONE.
//!
//! Ableton / REAPER / generic heuristics identify project folders without
//! mutating DAW project files. Ambiguous layouts fail instead of guessing.

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{Error, ErrorKind, Result};

/// How the project root was resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectKind {
    /// Directory / file associated with an Ableton Live Set (`.als`).
    Ableton,
    /// Directory / file associated with a REAPER project (`.rpp`).
    Reaper,
    /// Generic music project directory.
    Generic,
}

impl ProjectKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ableton => "Ableton",
            Self::Reaper => "REAPER",
            Self::Generic => "generic",
        }
    }
}

impl std::fmt::Display for ProjectKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedProjectRoot {
    pub root: PathBuf,
    pub kind: ProjectKind,
    /// Primary DAW project file (`.als` or `.rpp`) when known.
    pub primary_project_file: Option<PathBuf>,
}

impl ResolvedProjectRoot {
    /// Back-compat alias for Ableton `.als` path.
    pub fn primary_als(&self) -> Option<&PathBuf> {
        self.primary_project_file.as_ref().filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .map(|e| e.eq_ignore_ascii_case("als"))
                .unwrap_or(false)
        })
    }
}

/// Resolve the intended project root from a directory or DAW project file path.
pub fn resolve_project_root(path: &Path) -> Result<ResolvedProjectRoot> {
    let path = normalize_existing(path)?;
    if path.is_file() {
        return resolve_from_file(&path);
    }
    if path.is_dir() {
        return resolve_from_directory(&path);
    }
    Err(Error::new(ErrorKind::InvalidProject, "path is not a file or directory").with_path(path))
}

fn normalize_existing(path: &Path) -> Result<PathBuf> {
    if !path.exists() {
        return Err(Error::new(ErrorKind::NotFound, "project path does not exist").with_path(path));
    }
    match fs::canonicalize(path) {
        Ok(p) => Ok(strip_extended_prefix(p)),
        Err(_) => Ok(path.to_path_buf()),
    }
}

pub(crate) fn strip_extended_prefix(path: PathBuf) -> PathBuf {
    let s = path.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\") {
        PathBuf::from(rest)
    } else {
        path
    }
}

fn extension_eq(path: &Path, ext: &str) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case(ext))
        .unwrap_or(false)
}

fn resolve_from_file(file: &Path) -> Result<ResolvedProjectRoot> {
    if extension_eq(file, "als") {
        let parent = file.parent().ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidProject,
                "als file has no parent directory",
            )
            .with_path(file)
        })?;
        let root = normalize_existing(parent)?;
        return Ok(ResolvedProjectRoot {
            root,
            kind: ProjectKind::Ableton,
            primary_project_file: Some(file.to_path_buf()),
        });
    }
    if extension_eq(file, "rpp") {
        let parent = file.parent().ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidProject,
                "rpp file has no parent directory",
            )
            .with_path(file)
        })?;
        let root = normalize_existing(parent)?;
        return Ok(ResolvedProjectRoot {
            root,
            kind: ProjectKind::Reaper,
            primary_project_file: Some(file.to_path_buf()),
        });
    }
    // Ignore backup naming accidentally passed as primary
    if file
        .file_name()
        .and_then(|n| n.to_str())
        .map(|n| {
            n.to_ascii_lowercase().contains(".rpp-bak")
                || n.to_ascii_lowercase().ends_with(".rpp.bak")
        })
        .unwrap_or(false)
    {
        return Err(Error::new(
            ErrorKind::InvalidProject,
            "REAPER backup files (.rpp-bak) are not a canonical project; pass the .rpp file",
        )
        .with_path(file));
    }
    Err(Error::new(
        ErrorKind::InvalidProject,
        "file path must be an Ableton .als, REAPER .rpp, or a project directory",
    )
    .with_path(file))
}

fn resolve_from_directory(dir: &Path) -> Result<ResolvedProjectRoot> {
    let als_files = list_root_ext(dir, "als")?;
    let rpp_files = list_root_rpp(dir)?;

    if !als_files.is_empty() && !rpp_files.is_empty() {
        return Err(Error::new(
            ErrorKind::InvalidProject,
            "ambiguous project root: both Ableton .als and REAPER .rpp present; pass a specific project file",
        )
        .with_path(dir));
    }

    if rpp_files.len() > 1 {
        return Err(Error::new(
            ErrorKind::InvalidProject,
            format!(
                "ambiguous REAPER project root: {} .rpp files at top level; pass a specific .rpp path",
                rpp_files.len()
            ),
        )
        .with_path(dir));
    }
    if rpp_files.len() == 1 {
        return Ok(ResolvedProjectRoot {
            root: dir.to_path_buf(),
            kind: ProjectKind::Reaper,
            primary_project_file: Some(rpp_files[0].clone()),
        });
    }

    if als_files.len() > 1 {
        return Err(Error::new(
            ErrorKind::InvalidProject,
            format!(
                "ambiguous Ableton project root: {} .als files at top level; pass a specific .als path",
                als_files.len()
            ),
        )
        .with_path(dir));
    }
    if als_files.len() == 1 {
        return Ok(ResolvedProjectRoot {
            root: dir.to_path_buf(),
            kind: ProjectKind::Ableton,
            primary_project_file: Some(als_files[0].clone()),
        });
    }

    let nested_rpp = list_nested_single_ext_projects(dir, "rpp")?;
    let nested_als = list_nested_single_ext_projects(dir, "als")?;
    if nested_rpp.len() + nested_als.len() == 1 {
        let (kind, path) = if nested_rpp.len() == 1 {
            (ProjectKind::Reaper, &nested_rpp[0])
        } else {
            (ProjectKind::Ableton, &nested_als[0])
        };
        return Err(Error::new(
            ErrorKind::InvalidProject,
            format!(
                "directory looks like a parent folder; {kind} project appears to be {}",
                path.display()
            ),
        )
        .with_path(dir));
    }
    if nested_rpp.len() + nested_als.len() > 1 {
        return Err(Error::new(
            ErrorKind::InvalidProject,
            "ambiguous project root: multiple nested DAW projects detected",
        )
        .with_path(dir));
    }

    Ok(ResolvedProjectRoot {
        root: dir.to_path_buf(),
        kind: ProjectKind::Generic,
        primary_project_file: None,
    })
}

fn list_root_ext(dir: &Path, ext: &str) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for entry in
        fs::read_dir(dir).map_err(|e| Error::io("failed to read project directory", dir, e))?
    {
        let entry = entry.map_err(|e| Error::io("failed to read directory entry", dir, e))?;
        let path = entry.path();
        if path.is_file() && extension_eq(&path, ext) {
            out.push(path);
        }
    }
    out.sort();
    Ok(out)
}

fn list_root_rpp(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for entry in
        fs::read_dir(dir).map_err(|e| Error::io("failed to read project directory", dir, e))?
    {
        let entry = entry.map_err(|e| Error::io("failed to read directory entry", dir, e))?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        // Do not treat backups as canonical projects.
        if name.ends_with(".rpp-bak") || name.ends_with(".rpp.bak") || name.contains(".rpp-bak") {
            continue;
        }
        if extension_eq(&path, "rpp") {
            out.push(path);
        }
    }
    out.sort();
    Ok(out)
}

fn list_nested_single_ext_projects(dir: &Path, ext: &str) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for entry in
        fs::read_dir(dir).map_err(|e| Error::io("failed to read project directory", dir, e))?
    {
        let entry = entry.map_err(|e| Error::io("failed to read directory entry", dir, e))?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == ".git" || name == ".versione" {
            continue;
        }
        let files = if ext == "rpp" {
            list_root_rpp(&path)?
        } else {
            list_root_ext(&path, ext)?
        };
        if files.len() == 1 {
            out.push(path);
        }
    }
    out.sort();
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn resolves_directory_with_als() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("Track.als"), b"als").unwrap();
        let resolved = resolve_project_root(dir.path()).unwrap();
        assert_eq!(resolved.kind, ProjectKind::Ableton);
        assert!(resolved.primary_project_file.is_some());
    }

    #[test]
    fn resolves_from_als_file() {
        let dir = tempdir().unwrap();
        let als = dir.path().join("Song.als");
        fs::write(&als, b"als").unwrap();
        let resolved = resolve_project_root(&als).unwrap();
        let expected = strip_extended_prefix(fs::canonicalize(dir.path()).unwrap());
        assert_eq!(resolved.root, expected);
        assert_eq!(resolved.kind, ProjectKind::Ableton);
    }

    #[test]
    fn rejects_ambiguous_multiple_als() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("A.als"), b"a").unwrap();
        fs::write(dir.path().join("B.als"), b"b").unwrap();
        let err = resolve_project_root(dir.path()).unwrap_err();
        assert_eq!(err.kind, ErrorKind::InvalidProject);
    }

    #[test]
    fn resolves_rpp_file() {
        let dir = tempdir().unwrap();
        let rpp = dir.path().join("Song.rpp");
        fs::write(&rpp, b"<REAPER_PROJECT\n>\n").unwrap();
        let resolved = resolve_project_root(&rpp).unwrap();
        assert_eq!(resolved.kind, ProjectKind::Reaper);
    }

    #[test]
    fn resolves_directory_with_one_rpp() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("Song.rpp"), b"<REAPER_PROJECT\n>\n").unwrap();
        let resolved = resolve_project_root(dir.path()).unwrap();
        assert_eq!(resolved.kind, ProjectKind::Reaper);
    }

    #[test]
    fn rejects_ambiguous_multiple_rpp() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("A.rpp"), b"<REAPER_PROJECT\n>\n").unwrap();
        fs::write(dir.path().join("B.rpp"), b"<REAPER_PROJECT\n>\n").unwrap();
        let err = resolve_project_root(dir.path()).unwrap_err();
        assert_eq!(err.kind, ErrorKind::InvalidProject);
    }

    #[test]
    fn ignores_rpp_bak_when_listing() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("Song.rpp"), b"<REAPER_PROJECT\n>\n").unwrap();
        fs::write(dir.path().join("Song.rpp-bak"), b"bak").unwrap();
        let resolved = resolve_project_root(dir.path()).unwrap();
        assert_eq!(resolved.kind, ProjectKind::Reaper);
    }

    #[test]
    fn unicode_rpp_name() {
        let dir = tempdir().unwrap();
        let rpp = dir.path().join("Песня.rpp");
        fs::write(&rpp, b"<REAPER_PROJECT\n>\n").unwrap();
        let resolved = resolve_project_root(&rpp).unwrap();
        assert_eq!(resolved.kind, ProjectKind::Reaper);
    }

    #[test]
    fn mixed_case_rpp_extension() {
        let dir = tempdir().unwrap();
        let rpp = dir.path().join("Song.RPP");
        fs::write(&rpp, b"<REAPER_PROJECT\n>\n").unwrap();
        let resolved = resolve_project_root(&rpp).unwrap();
        assert_eq!(resolved.kind, ProjectKind::Reaper);
    }
}
